//! Shape admission before Wasmi validation/translation; not a replacement validator.
use wasmparser::{BinaryReader, Encoding, ExternalKind, Operator, Parser, Payload, ValType};
fn scalar(value: ValType) -> Result<(), String> {
    if matches!(value, ValType::I32 | ValType::I64) {
        Ok(())
    } else {
        Err("integer types only".into())
    }
}
fn count(actual: u32, maximum: u32) -> Result<(), String> {
    if actual <= maximum {
        Ok(())
    } else {
        Err("declaration limit".into())
    }
}
pub(super) fn check(wasm: &[u8]) -> Result<(), String> {
    if wasm.len() > 65536 {
        return Err("module byte limit".into());
    }
    let mut types = Vec::new();
    let mut functions = Vec::new();
    let mut evaluate = None;
    let mut code_count = 0;
    let mut bodies = 0;
    let mut operators = 0;
    let mut seen = std::collections::BTreeSet::new();
    for payload in Parser::new(0).parse_all(wasm) {
        let payload = payload.map_err(|_| "malformed module".to_string())?;
        if let Some((id, _)) = payload.as_section() {
            if id != 0 && !seen.insert(id) {
                return Err("duplicate section".into());
            }
        }
        match payload {
            Payload::Version {
                num: 1,
                encoding: Encoding::Module,
                ..
            } => {}
            Payload::TypeSection(reader) => {
                count(reader.count(), 32)?;
                // Parse only flat MVP function types. Reject recursive/GC groups
                // before an allocating type decoder sees their declared counts.
                let start = reader.original_position();
                let bytes = wasm
                    .get(start..reader.range().end)
                    .ok_or("malformed type range")?;
                let mut raw = BinaryReader::new(bytes, start);
                for _ in 0..reader.count() {
                    if raw.read_u8().map_err(|_| "malformed type")? != 0x60 {
                        return Err("function types only".into());
                    }
                    let mut signature = Vec::new();
                    for maximum in [8, 1] {
                        let length = raw.read_var_u32().map_err(|_| "malformed type")?;
                        count(length, maximum)?;
                        let mut values = Vec::new();
                        for _ in 0..length {
                            let value = raw.read_u8().map_err(|_| "malformed type")?;
                            if !matches!(value, 0x7f | 0x7e) {
                                return Err("integer types only".into());
                            }
                            values.push(value);
                        }
                        signature.push(values);
                    }
                    types.push(signature);
                }
                if !raw.eof() {
                    return Err("trailing type bytes".into());
                }
            }
            Payload::ImportSection(reader) => {
                if reader.count() != 0 {
                    return Err("imports forbidden".into());
                }
            }
            Payload::FunctionSection(reader) => {
                count(reader.count(), 64)?;
                for value in reader {
                    functions.push(value.map_err(|_| "malformed function")?);
                }
            }
            Payload::TableSection(reader) => count(reader.count(), 0)?,
            Payload::ElementSection(reader) => count(reader.count(), 0)?,
            Payload::MemorySection(reader) => {
                count(reader.count(), 1)?;
                for memory in reader {
                    let memory = memory.map_err(|_| "malformed memory")?;
                    if memory.memory64
                        || memory.shared
                        || memory.page_size_log2.is_some()
                        || memory.initial > 1
                        || memory.maximum.is_none_or(|maximum| maximum > 1)
                    {
                        return Err("memory declaration limit".into());
                    }
                }
            }
            Payload::GlobalSection(reader) => {
                count(reader.count(), 16)?;
                for global in reader {
                    scalar(global.map_err(|_| "malformed global")?.ty.content_type)?;
                }
            }
            Payload::ExportSection(reader) => {
                count(reader.count(), 4)?;
                for export in reader {
                    let export = export.map_err(|_| "malformed export")?;
                    if export.name.len() > 64 {
                        return Err("export name limit".into());
                    }
                    if export.name == "evaluate"
                        && (export.kind != ExternalKind::Func
                            || evaluate.replace(export.index).is_some())
                    {
                        return Err("invalid evaluate export".into());
                    }
                }
            }
            Payload::CodeSectionStart { count: entries, .. } => {
                count(entries, 64)?;
                code_count = entries;
            }
            Payload::CodeSectionEntry(body) => {
                bodies += 1;
                let locals = body.get_locals_reader().map_err(|_| "malformed locals")?;
                count(locals.get_count(), 32)?;
                let mut total: u32 = 0;
                for local in locals {
                    let (number, ty) = local.map_err(|_| "malformed locals")?;
                    total = total.checked_add(number).ok_or("local count overflow")?;
                    count(total, 128)?;
                    scalar(ty)?;
                }
                let mut ops = body
                    .get_operators_reader()
                    .map_err(|_| "malformed operators")?;
                let mut depth: u32 = 0;
                while !ops.eof() {
                    operators += 1;
                    count(operators, 8192)?;
                    match ops.read().map_err(|_| "malformed operator")? {
                        Operator::Block { .. } | Operator::Loop { .. } | Operator::If { .. } => {
                            depth += 1;
                            count(depth, 32)?;
                        }
                        Operator::End => depth = depth.saturating_sub(1),
                        _ => {}
                    }
                }
            }
            Payload::DataCountSection { count: entries, .. } => count(entries, 1)?,
            Payload::DataSection(reader) => count(reader.count(), 1)?,
            Payload::StartSection { .. } | Payload::CustomSection(_) | Payload::End(_) => {}
            _ => return Err("unsupported module encoding or section".into()),
        }
    }
    if functions.len() != code_count as usize || bodies != code_count {
        return Err("function/code mismatch".into());
    }
    let function = functions
        .get(evaluate.ok_or("missing evaluate export")? as usize)
        .ok_or("invalid evaluate index")?;
    let signature = types
        .get(*function as usize)
        .ok_or("invalid function type")?;
    if signature[0] != [0x7e] || signature[1] != [0x7e] {
        return Err("evaluate requires i64 -> i64".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::check;
    fn module(extra: &str, body: &str) -> Vec<u8> {
        wat::parse_str(format!(
            "(module {extra} (func (export \"evaluate\") (param i64) (result i64) {body}))"
        ))
        .unwrap()
    }
    #[test]
    fn declaration_and_local_boundaries() {
        for (allowed, denied) in [
            (
                (0..31)
                    .map(|i| format!("(type $t{i} (func))"))
                    .collect::<String>(),
                (0..32).map(|i| format!("(type $t{i} (func))")).collect(),
            ),
            ("(func)".repeat(63), "(func)".repeat(64)),
            (
                "(global i64 (i64.const 0))".repeat(16),
                "(global i64 (i64.const 0))".repeat(17),
            ),
        ] {
            assert!(check(&module(&allowed, "local.get 0")).is_ok());
            assert_eq!(
                check(&module(&denied, "local.get 0")).unwrap_err(),
                "declaration limit"
            );
        }
        let locals = |n| format!("(local {}) local.get 0", "i64 ".repeat(n));
        assert!(check(&module("", &locals(128))).is_ok());
        assert_eq!(
            check(&module("", &locals(129))).unwrap_err(),
            "declaration limit"
        );
    }
    #[test]
    fn tiny_encoded_local_amplification_is_rejected() {
        let mut bytes = vec![
            0, 97, 115, 109, 1, 0, 0, 0, 1, 6, 1, 0x60, 1, 0x7e, 1, 0x7e, 3, 2, 1, 0, 7, 12, 1, 8,
        ];
        bytes.extend_from_slice(b"evaluate");
        bytes.extend_from_slice(&[
            0, 0, 10, 12, 1, 10, 1, 0xff, 0xff, 0xff, 0xff, 0x0f, 0x7e, 0x20, 0, 0x0b,
        ]);
        assert_eq!(bytes.len(), 48);
        assert_eq!(check(&bytes).unwrap_err(), "declaration limit");
    }
    #[test]
    fn code_depth_and_total_operators_bounded() {
        let nested = |n| format!("{}{} local.get 0", "(block ".repeat(n), ")".repeat(n));
        assert!(check(&module("", &nested(32))).is_ok());
        assert_eq!(
            check(&module("", &nested(33))).unwrap_err(),
            "declaration limit"
        );
        assert!(check(&module("", &format!("{}local.get 0", "nop ".repeat(8190)))).is_ok());
        assert_eq!(
            check(&module("", &format!("{}local.get 0", "nop ".repeat(8191)))).unwrap_err(),
            "declaration limit"
        );
    }
    #[test]
    fn memory_import_abi_and_full_validator_are_distinct() {
        assert!(check(&module("(memory 1 1)", "local.get 0")).is_ok());
        for memory in ["(memory 1)", "(memory 1 2)", "(memory i64 1 1)"] {
            assert_eq!(
                check(&module(memory, "local.get 0")).unwrap_err(),
                "memory declaration limit"
            );
        }
        let wrong=wat::parse_str("(module (func $init (loop $l br $l)) (start $init) (func (export \"evaluate\") (param i32) (result i32) local.get 0))").unwrap();
        assert_eq!(
            crate::evaluate_i64(&wrong, 0).unwrap_err(),
            "evaluate requires i64 -> i64"
        );
        let invalid = module("", "i32.const 1");
        assert!(check(&invalid).is_ok());
        assert!(
            crate::evaluate_i64(&invalid, 0).is_err(),
            "Shape admission must retain full type validation"
        );
    }
}
