use anyhow::{bail, Context, Result};
use clap::Parser;
use cogfabric::{chat_client, config::Config};
use std::{collections::VecDeque, io::Write, os::fd::RawFd, path::PathBuf, time::Duration};

const MAX_SESSION_USER_BYTES: usize = 32 * 1024;
const MAX_OUTPUT_RESERVATION: usize = 4096;

#[derive(Parser)]
#[command(
    name = "cogfabric-chat",
    version,
    about = "Bounded, tool-free Cognitive Fabric chat"
)]
struct Args {
    /// Cognitive Fabric HTTP server base URL. Must be numeric loopback or localhost.
    #[arg(long, default_value = "http://127.0.0.1:8234/v1")]
    server: String,
    /// Private Cognitive Fabric settings directory.
    #[arg(long)]
    home: Option<PathBuf>,
    /// Maximum number of chat turns in this process.
    #[arg(long, default_value_t = 16)]
    max_turns: usize,
    /// Reserved model output tokens per turn.
    #[arg(long, default_value_t = 256)]
    max_tokens: usize,
    /// Maximum wall time for the interactive session in seconds.
    #[arg(long, default_value_t = 3600, value_parser = clap::value_parser!(u64).range(60..=28_800))]
    session_seconds: u64,
}

enum InputLine {
    End,
    Text(String),
    TooLong,
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("cogfabric-chat: {error:#}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let args = Args::parse();
    if !(1..=16).contains(&args.max_turns) || !(16..=512).contains(&args.max_tokens) {
        bail!("Chat limits require 1–16 turns and 16–512 output tokens per turn")
    }
    let reserved = args.max_turns.saturating_mul(args.max_tokens);
    if reserved > MAX_OUTPUT_RESERVATION {
        bail!("The total reserved output budget must be at most {MAX_OUTPUT_RESERVATION} tokens")
    }
    let config = Config::load(args.home, None)?;
    let timeout = Duration::from_secs(config.timeout_seconds.min(120));
    let client = chat_client::ClientApi::new(&args.server, timeout, args.max_tokens)?;
    match tokio::time::timeout(
        Duration::from_secs(args.session_seconds),
        run_session(client, config.temperature, args.max_turns),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => {
            println!("\nSession time limit reached.");
            Ok(())
        }
    }
}

async fn run_session(
    client: chat_client::ClientApi,
    temperature: f64,
    max_turns: usize,
) -> Result<()> {
    let mut input = NonblockingInput::new()?;
    let mut transcript = Vec::<chat_client::Message>::new();
    let mut total_user_bytes = 0usize;
    let mut turns = 0usize;

    println!(
        "Cognitive Fabric chat (model: {}). Type /help for commands.",
        chat_client::MODEL_ALIAS
    );
    loop {
        print!("you> ");
        std::io::stdout().flush()?;
        let line = tokio::select! {
            result = read_bounded_line(&mut input, chat_client::MAX_MESSAGE_BYTES) => result?,
            _ = tokio::signal::ctrl_c() => {
                println!("\nGoodbye.");
                return Ok(());
            }
        };
        let text = match line {
            InputLine::End => {
                println!();
                return Ok(());
            }
            InputLine::TooLong => {
                println!("Input is too long; one message is limited to 8 KiB.");
                continue;
            }
            InputLine::Text(text) => text,
        };
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        match text {
            "/quit" | "/exit" => {
                println!("Goodbye.");
                return Ok(());
            }
            "/help" => {
                println!("Commands: /help, /reset, /quit. Each chat turn is text-only; no tools are advertised or run.");
                continue;
            }
            "/reset" => {
                transcript.clear();
                println!("Conversation context cleared. Session input and token budgets remain in effect.");
                continue;
            }
            _ => {}
        }
        if turns >= max_turns {
            println!("Session turn limit reached.");
            return Ok(());
        }
        let input_bytes = text.len();
        if input_bytes > chat_client::MAX_MESSAGE_BYTES
            || total_user_bytes.saturating_add(input_bytes) > MAX_SESSION_USER_BYTES
        {
            println!("Session input limit reached (32 KiB total user text).");
            return Ok(());
        }

        total_user_bytes += input_bytes;
        turns += 1;
        transcript.push(chatfabric_user_message(text));
        trim_transcript(&mut transcript)?;

        let completion = tokio::select! {
            result = client.complete(&transcript, temperature) => Some(result),
            _ = tokio::signal::ctrl_c() => None,
        };
        let Some(completion) = completion else {
            println!("\nGoodbye.");
            return Ok(());
        };
        match completion {
            Ok(reply) => {
                println!("assistant> {}", safe_terminal_text(&reply));
                transcript.push(chatfabric_assistant_message(&reply));
                trim_transcript(&mut transcript)?;
            }
            Err(error) => {
                transcript.pop();
                eprintln!("chat request failed: {error}");
            }
        }
    }
}

/// Prevent untrusted model text from sending terminal control and bidi commands.
fn safe_terminal_text(text: &str) -> String {
    use std::fmt::Write as _;
    let mut safe = String::with_capacity(text.len());
    for character in text.chars() {
        let code = character as u32;
        let bidi_control = matches!(code, 0x202A..=0x202E | 0x2066..=0x2069);
        if character == '\n' || character == '\t' {
            safe.push(character);
        } else if character.is_control() || bidi_control {
            let _ = write!(safe, "\\u{{{code:x}}}");
        } else {
            safe.push(character);
        }
    }
    safe
}

fn chatfabric_user_message(content: &str) -> chat_client::Message {
    chat_client::Message {
        role: chat_client::Role::User,
        content: content.to_owned(),
    }
}

fn chatfabric_assistant_message(content: &str) -> chat_client::Message {
    chat_client::Message {
        role: chat_client::Role::Assistant,
        content: content.to_owned(),
    }
}

/// Retains the newest complete exchanges and never splits a user/assistant pair.
fn trim_transcript(transcript: &mut Vec<chat_client::Message>) -> Result<()> {
    while transcript.len() > 32
        || serde_json::to_vec(transcript)?.len() > chat_client::MAX_TRANSCRIPT_BYTES
    {
        if transcript.len() < 3
            || !matches!(
                transcript.first().map(|message| message.role),
                Some(chat_client::Role::User)
            )
            || !matches!(
                transcript.get(1).map(|message| message.role),
                Some(chat_client::Role::Assistant)
            )
        {
            bail!("The current chat exchange exceeds the transcript limit; use /reset to continue")
        }
        transcript.drain(..2);
    }
    Ok(())
}

/// Reads one UTF-8 line while keeping memory use bounded even for pasted input.
/// Nonblocking descriptor reads keep Ctrl-C and session expiry responsive even
/// when no newline is entered. Original descriptor flags are restored on drop.
struct NonblockingInput {
    fd: RawFd,
    original_flags: libc::c_int,
    pending: VecDeque<u8>,
    eof: bool,
}

impl NonblockingInput {
    fn new() -> Result<Self> {
        let fd = libc::STDIN_FILENO;
        // SAFETY: fcntl reads and updates flags on the process's standard input.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 {
            return Err(std::io::Error::last_os_error())
                .context("Could not inspect standard input");
        }
        // SAFETY: only O_NONBLOCK is added; Drop restores the original flags.
        if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return Err(std::io::Error::last_os_error())
                .context("Could not configure standard input");
        }
        Ok(Self {
            fd,
            original_flags: flags,
            pending: VecDeque::new(),
            eof: false,
        })
    }

    async fn next_byte(&mut self) -> Result<Option<u8>> {
        loop {
            if let Some(byte) = self.pending.pop_front() {
                return Ok(Some(byte));
            }
            if self.eof {
                return Ok(None);
            }
            let mut chunk = [0u8; 1024];
            // SAFETY: chunk is writable for its full length and fd remains open
            // for the duration of this client process.
            let count = unsafe { libc::read(self.fd, chunk.as_mut_ptr().cast(), chunk.len()) };
            if count > 0 {
                self.pending.extend(&chunk[..count as usize]);
                tokio::task::yield_now().await;
                continue;
            }
            if count == 0 {
                self.eof = true;
                return Ok(None);
            }
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            if error.kind() == std::io::ErrorKind::WouldBlock {
                tokio::time::sleep(Duration::from_millis(20)).await;
                continue;
            }
            return Err(error).context("Could not read standard input");
        }
    }
}

impl Drop for NonblockingInput {
    fn drop(&mut self) {
        // SAFETY: restore the exact file status flags captured in new().
        let _ = unsafe { libc::fcntl(self.fd, libc::F_SETFL, self.original_flags) };
    }
}

async fn read_bounded_line(reader: &mut NonblockingInput, maximum: usize) -> Result<InputLine> {
    let mut bytes = Vec::with_capacity(maximum.min(1024));
    let mut too_long = false;
    let mut saw_bytes = false;
    loop {
        let Some(byte) = reader.next_byte().await? else {
            if !saw_bytes {
                return Ok(InputLine::End);
            }
            break;
        };
        if byte == b'\n' {
            break;
        }
        saw_bytes = true;
        if !too_long {
            if bytes.len() == maximum {
                too_long = true;
                bytes.clear();
            } else {
                bytes.push(byte);
            }
        }
    }
    if too_long {
        return Ok(InputLine::TooLong);
    }
    if bytes.last() == Some(&b'\r') {
        bytes.pop();
    }
    let text = String::from_utf8(bytes).context("Input must be valid UTF-8")?;
    Ok(InputLine::Text(text))
}
