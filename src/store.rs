//! Persistent task, event, document and candidate repository.
use anyhow::{anyhow, bail, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS tasks (
 id TEXT PRIMARY KEY, query TEXT NOT NULL, mode TEXT NOT NULL,
 status TEXT NOT NULL, answer TEXT NOT NULL DEFAULT '', created REAL NOT NULL, updated REAL NOT NULL
);
CREATE TABLE IF NOT EXISTS events (
 id INTEGER PRIMARY KEY AUTOINCREMENT, task_id TEXT NOT NULL,
 kind TEXT NOT NULL, source TEXT NOT NULL, payload TEXT NOT NULL, created REAL NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_events_task ON events(task_id, id);
CREATE TABLE IF NOT EXISTS skills (
 id TEXT PRIMARY KEY, description TEXT NOT NULL, tags_json TEXT NOT NULL,
 active_version INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS documents (
 id TEXT PRIMARY KEY, skill_id TEXT NOT NULL, title TEXT NOT NULL,
 body TEXT NOT NULL, source TEXT NOT NULL, license TEXT NOT NULL,
 sha256 TEXT NOT NULL UNIQUE, FOREIGN KEY(skill_id) REFERENCES skills(id)
);
CREATE VIRTUAL TABLE IF NOT EXISTS document_index USING fts5(title, body, content='documents', content_rowid='rowid');
CREATE TRIGGER IF NOT EXISTS documents_insert AFTER INSERT ON documents BEGIN
 INSERT INTO document_index(rowid,title,body) VALUES (new.rowid,new.title,new.body);
END;
CREATE TABLE IF NOT EXISTS candidates (
 id TEXT PRIMARY KEY, skill_id TEXT NOT NULL, kind TEXT NOT NULL,
 version INTEGER NOT NULL, artifact TEXT NOT NULL, checksum TEXT NOT NULL,
 status TEXT NOT NULL, source TEXT NOT NULL, license TEXT NOT NULL,
 created REAL NOT NULL, UNIQUE(skill_id,version), FOREIGN KEY(skill_id) REFERENCES skills(id)
);
CREATE TABLE IF NOT EXISTS evaluations (
 id INTEGER PRIMARY KEY AUTOINCREMENT, candidate_id TEXT NOT NULL,
 suite TEXT NOT NULL, success INTEGER NOT NULL, score REAL NOT NULL,
 details TEXT NOT NULL, created REAL NOT NULL,
 FOREIGN KEY(candidate_id) REFERENCES candidates(id)
);
"#;

fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}
pub fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Skill {
    pub id: String,
    pub description: String,
    pub tags: Vec<String>,
    pub active_version: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Document {
    pub id: String,
    pub skill_id: String,
    pub title: String,
    pub body: String,
    pub source: String,
    pub license: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Candidate {
    pub id: String,
    pub skill_id: String,
    pub kind: String,
    pub version: i64,
    pub artifact: String,
    pub checksum: String,
    pub status: String,
    pub source: String,
    pub license: String,
}

pub struct Store {
    db: Mutex<Connection>,
}
impl Store {
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.db
            .lock()
            .map_err(|_| anyhow!("Database mutex poisoned"))
    }
    pub fn open(path: &Path) -> Result<Self> {
        if path.is_symlink() {
            bail!("Database path cannot be a symlink")
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let db = Connection::open(path)?;
        db.pragma_update(None, "foreign_keys", "ON")?;
        db.pragma_update(None, "journal_mode", "WAL")?;
        db.execute_batch(SCHEMA)?;
        Ok(Self { db: Mutex::new(db) })
    }
    pub fn create_task(&self, query: &str, mode: &str) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        self.lock()?.execute(
            "INSERT INTO tasks VALUES (?1,?2,?3,'running','',?4,?4)",
            params![id, query, mode, now()],
        )?;
        Ok(id)
    }
    pub fn finish_task(&self, id: &str, status: &str, answer: &str) -> Result<()> {
        self.lock()?.execute(
            "UPDATE tasks SET status=?2,answer=?3,updated=?4 WHERE id=?1",
            params![id, status, answer, now()],
        )?;
        Ok(())
    }
    pub fn event(&self, id: &str, kind: &str, source: &str, payload: Value) -> Result<()> {
        self.lock()?.execute(
            "INSERT INTO events(task_id,kind,source,payload,created) VALUES (?1,?2,?3,?4,?5)",
            params![id, kind, source, payload.to_string(), now()],
        )?;
        Ok(())
    }
    pub fn task(&self, id: &str) -> Result<Value> {
        let db = self.lock()?;
        let task:Option<Value>=db.query_row("SELECT id,query,mode,status,answer FROM tasks WHERE id=?1",[id],|r| {
            Ok(json!({"id":r.get::<_,String>(0)?,"query":r.get::<_,String>(1)?,"mode":r.get::<_,String>(2)?,
                "status":r.get::<_,String>(3)?,"answer":r.get::<_,String>(4)?}))
        }).optional()?;
        let mut stmt =
            db.prepare("SELECT kind,source,payload FROM events WHERE task_id=?1 ORDER BY id")?;
        let events = stmt
            .query_map([id], |r| {
                let payload: String = r.get(2)?;
                Ok(
                    json!({"kind":r.get::<_,String>(0)?,"source":r.get::<_,String>(1)?,
                 "payload":serde_json::from_str::<Value>(&payload).unwrap_or(Value::Null)}),
                )
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(json!({"task":task,"events":events}))
    }
    pub fn ensure_skill(&self, id: &str, description: &str, tags: &[String]) -> Result<()> {
        self.lock()?.execute(
            "INSERT OR IGNORE INTO skills(id,description,tags_json) VALUES (?1,?2,?3)",
            params![id, description, serde_json::to_string(tags)?],
        )?;
        Ok(())
    }
    pub fn skills(&self) -> Result<Vec<Skill>> {
        let db = self.lock()?;
        let mut stmt =
            db.prepare("SELECT id,description,tags_json,active_version FROM skills ORDER BY id")?;
        let entries = stmt
            .query_map([], |r| {
                let tags: String = r.get(2)?;
                Ok(Skill {
                    id: r.get(0)?,
                    description: r.get(1)?,
                    tags: serde_json::from_str(&tags).unwrap_or_default(),
                    active_version: r.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(entries)
    }
    pub fn add_document(&self, doc: &Document) -> Result<bool> {
        let result = self.lock()?.execute(
            "INSERT OR IGNORE INTO documents VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![
                doc.id,
                doc.skill_id,
                doc.title,
                doc.body,
                doc.source,
                doc.license,
                sha256(doc.body.as_bytes())
            ],
        )?;
        Ok(result != 0)
    }
    pub fn search(&self, query: &str, skill: Option<&str>, limit: usize) -> Result<Vec<Document>> {
        let mut tokens = Vec::new();
        for token in query.split(|c: char| !c.is_ascii_alphanumeric() && c != '_') {
            if token.len() >= 2 && token.len() <= 40 && tokens.len() < 8 {
                tokens.push(format!("\"{}\"", token))
            }
        }
        if tokens.is_empty() {
            return Ok(Vec::new());
        }
        let match_query = tokens.join(" OR ");
        let db = self.lock()?;
        let statement = "SELECT d.id,d.skill_id,d.title,d.body,d.source,d.license \
           FROM document_index JOIN documents d ON d.rowid=document_index.rowid \
           WHERE document_index MATCH ?1 AND (?2 IS NULL OR d.skill_id=?2) \
           ORDER BY bm25(document_index) LIMIT ?3";
        let mut stmt = db.prepare(statement)?;
        let rows = stmt
            .query_map(
                params![match_query, skill, limit.clamp(1, 20) as i64],
                |r| {
                    Ok(Document {
                        id: r.get(0)?,
                        skill_id: r.get(1)?,
                        title: r.get(2)?,
                        body: r.get(3)?,
                        source: r.get(4)?,
                        license: r.get(5)?,
                    })
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
    pub fn create_candidate(
        &self,
        skill: &str,
        kind: &str,
        artifact: &str,
        checksum: &str,
        source: &str,
        license: &str,
    ) -> Result<Candidate> {
        if kind != "rag" && kind != "lora" {
            bail!("Unsupported artifact kind")
        }
        let db = self.lock()?;
        let exists: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM skills WHERE id=?1)",
            [skill],
            |r| r.get(0),
        )?;
        if !exists {
            bail!("Skill does not exist")
        }
        let version: i64 = db.query_row(
            "SELECT COALESCE(MAX(version),0)+1 FROM candidates WHERE skill_id=?1",
            [skill],
            |r| r.get(0),
        )?;
        let candidate = Candidate {
            id: Uuid::new_v4().to_string(),
            skill_id: skill.into(),
            kind: kind.into(),
            version,
            artifact: artifact.into(),
            checksum: checksum.into(),
            status: "candidate".into(),
            source: source.into(),
            license: license.into(),
        };
        db.execute(
            "INSERT INTO candidates VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                candidate.id,
                candidate.skill_id,
                candidate.kind,
                candidate.version,
                candidate.artifact,
                candidate.checksum,
                candidate.status,
                candidate.source,
                candidate.license,
                now()
            ],
        )?;
        Ok(candidate)
    }
    pub fn candidate(&self, id: &str) -> Result<Candidate> {
        self.lock()?.query_row("SELECT id,skill_id,kind,version,artifact,checksum,status,source,license FROM candidates WHERE id=?1",
            [id],|r| Ok(Candidate {id:r.get(0)?,skill_id:r.get(1)?,kind:r.get(2)?,version:r.get(3)?,artifact:r.get(4)?,
                checksum:r.get(5)?,status:r.get(6)?,source:r.get(7)?,license:r.get(8)?}))
            .map_err(Into::into)
    }
    pub fn candidates(&self) -> Result<Vec<Candidate>> {
        let db = self.lock()?;
        let mut stmt=db.prepare("SELECT id,skill_id,kind,version,artifact,checksum,status,source,license FROM candidates ORDER BY created DESC")?;
        let entries = stmt
            .query_map([], |r| {
                Ok(Candidate {
                    id: r.get(0)?,
                    skill_id: r.get(1)?,
                    kind: r.get(2)?,
                    version: r.get(3)?,
                    artifact: r.get(4)?,
                    checksum: r.get(5)?,
                    status: r.get(6)?,
                    source: r.get(7)?,
                    license: r.get(8)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(entries)
    }
    pub fn evaluation(
        &self,
        id: &str,
        suite: &str,
        pass: bool,
        score: f64,
        details: Value,
    ) -> Result<()> {
        self.lock()?.execute("INSERT INTO evaluations(candidate_id,suite,success,score,details,created) VALUES (?1,?2,?3,?4,?5,?6)",
            params![id,suite,pass as i32,score,details.to_string(),now()])?;
        Ok(())
    }
    pub fn promote_rag(&self, id: &str, body: &str) -> Result<Candidate> {
        // Revalidate gates and candidate while holding the same DB lock.
        let mut db = self.lock()?;
        let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let candidate:Candidate=tx.query_row("SELECT id,skill_id,kind,version,artifact,checksum,status,source,license FROM candidates WHERE id=?1",[id],
            |r|Ok(Candidate{id:r.get(0)?,skill_id:r.get(1)?,kind:r.get(2)?,version:r.get(3)?,artifact:r.get(4)?,
                checksum:r.get(5)?,status:r.get(6)?,source:r.get(7)?,license:r.get(8)?}))?;
        if candidate.status != "candidate" || candidate.kind != "rag" {
            bail!("Only new RAG candidates may be promoted")
        }
        if sha256(body.as_bytes()) != candidate.checksum {
            bail!("Artifact changed since evaluation")
        }
        for suite in ["integrity", "quality"] {
            let approved:Option<(i64,f64)>=tx.query_row("SELECT success,score FROM evaluations WHERE candidate_id=?1 AND suite=?2 ORDER BY id DESC LIMIT 1",
                params![id,suite],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
            if !matches!(approved,Some((1,score)) if score>=0.9) {
                bail!("Missing passing {suite} evaluation")
            }
        }
        tx.execute(
            "INSERT OR IGNORE INTO documents VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![
                Uuid::new_v4().to_string(),
                candidate.skill_id,
                Path::new(&candidate.artifact)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("artifact"),
                body,
                candidate.source,
                candidate.license,
                candidate.checksum
            ],
        )?;
        tx.execute("UPDATE candidates SET status='approved' WHERE id=?1", [id])?;
        tx.execute(
            "UPDATE skills SET active_version=?2 WHERE id=?1",
            params![candidate.skill_id, candidate.version],
        )?;
        tx.commit()?;
        drop(db);
        self.candidate(id)
    }
}

pub fn bootstrap(store: &Store) -> Result<()> {
    let skills: &[(&str, &str, &[&str])] = &[
        (
            "rust.s1",
            "Rust language and tooling",
            &["rust", "cargo", "tokio", "borrow", "lifetime"],
        ),
        (
            "linux.s1",
            "Linux administration",
            &["linux", "arch", "nixos", "bash", "kernel"],
        ),
        (
            "security.s1",
            "Defensive software security",
            &["security", "audit", "vulnerability", "secure"],
        ),
        (
            "reverse.s1",
            "Binary format analysis",
            &["binary", "reverse", "disassembly", "metadata"],
        ),
        (
            "data.sql.s1",
            "SQL and databases",
            &["sql", "sqlite", "database", "query"],
        ),
        (
            "git.s1",
            "Git version control",
            &["git", "commit", "merge", "repository"],
        ),
        (
            "unity.s1",
            "Unity and C#",
            &["unity", "shader", "gameobject", "csharp", "urp"],
        ),
    ];
    for (id, description, tags) in skills {
        store.ensure_skill(
            id,
            description,
            &tags.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        )?;
    }
    Ok(())
}

pub fn route(skills: &[Skill], query: &str) -> Vec<String> {
    let words = query
        .to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .map(str::to_string)
        .collect::<std::collections::HashSet<_>>();
    let mut scores: Vec<(i64, String)> = skills
        .iter()
        .filter_map(|skill| {
            let value = skill.tags.iter().filter(|tag| words.contains(*tag)).count() as i64 * 3;
            if value > 0 {
                Some((value, skill.id.clone()))
            } else {
                None
            }
        })
        .collect();
    scores.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scores.into_iter().take(2).map(|(_, id)| id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn skill_routing() {
        let tmp = std::env::temp_dir().join(format!("cogfabric-test-{}.db", Uuid::new_v4()));
        let store = Store::open(&tmp).unwrap();
        bootstrap(&store).unwrap();
        assert_eq!(
            route(&store.skills().unwrap(), "Rust tokio example")[0],
            "rust.s1"
        );
        drop(store);
        let _ = std::fs::remove_file(tmp);
    }
}
