//! Experimental import-free scalar expert executor; not wired into runtime authority.
pub mod artifact;
pub mod audit;
pub mod authority;
#[cfg(feature = "core-audit")]
pub mod core_audit;
mod preflight;

fn compile_checked(wasm: &[u8]) -> Result<(wasmi::Engine, wasmi::Module), String> {
    if wasm.len() > 65536 {
        return Err("module byte limit".into());
    }
    preflight::check(wasm)?;
    let mut config = wasmi::Config::default();
    config
        .consume_fuel(true)
        .floats(false)
        .wasm_multi_memory(false)
        .wasm_reference_types(false)
        .wasm_tail_call(false)
        .wasm_extended_const(false);
    config
        .enforced_limits(wasmi::EnforcedLimits::strict())
        .set_max_recursion_depth(32)
        .set_min_stack_height(1024)
        .set_max_stack_height(65536)
        .set_max_cached_stacks(0);
    let engine = wasmi::Engine::new(&config);
    let module = wasmi::Module::new(&engine, wasm).map_err(|e| e.to_string())?;
    if module.imports().next().is_some() {
        return Err("imports forbidden".into());
    }
    Ok((engine, module))
}

fn call_checked(engine: &wasmi::Engine, module: &wasmi::Module, input: i64) -> Result<i64, String> {
    let limits = wasmi::StoreLimitsBuilder::new()
        .memory_size(65536)
        .memories(1)
        .tables(0)
        .table_elements(0)
        .instances(1)
        .trap_on_grow_failure(true)
        .build();
    let mut store = wasmi::Store::new(engine, limits);
    store.limiter(|limits| limits);
    store.set_fuel(1000).map_err(|e| e.to_string())?;
    let instance = wasmi::Linker::new(engine)
        .instantiate_and_start(&mut store, module)
        .map_err(|e| e.to_string())?;
    let function = instance
        .get_typed_func::<i64, i64>(&store, "evaluate")
        .map_err(|e| e.to_string())?;
    function.call(&mut store, input).map_err(|e| e.to_string())
}
#[cfg(test)]
fn evaluate_i64(wasm: &[u8], input: i64) -> Result<i64, String> {
    let (engine, module) = compile_checked(wasm)?;
    call_checked(&engine, &module, input)
}

#[test]
fn integer_guest() {
    let bytes=wat::parse_str("(module (func (export \"evaluate\") (param i64) (result i64) local.get 0 i64.const 2 i64.mul))").unwrap();
    assert_eq!(evaluate_i64(&bytes, 21).unwrap(), 42);
}
#[test]
fn imports_denied() {
    let bytes=wat::parse_str("(module (import \"wasi_snapshot_preview1\" \"fd_read\" (func)) (func (export \"evaluate\") (param i64) (result i64) local.get 0))").unwrap();
    assert_eq!(evaluate_i64(&bytes, 0).unwrap_err(), "imports forbidden");
}
#[test]
fn fuel_stops_loops_and_start_functions() {
    for text in ["(module (func (export \"evaluate\") (param i64) (result i64) (loop $l br $l) i64.const 0))", "(module (func $init (loop $l br $l)) (start $init) (func (export \"evaluate\") (param i64) (result i64) local.get 0))"] {
        let error=evaluate_i64(&wat::parse_str(text).unwrap(),0).unwrap_err();
        assert!(error.contains("fuel"),"{error}");
    }
}
#[test]
fn memory_and_tables_limited() {
    for text in ["(module (memory 2) (func (export \"evaluate\") (param i64) (result i64) local.get 0))", "(module (memory 1 2) (func (export \"evaluate\") (param i64) (result i64) i32.const 1 memory.grow drop local.get 0))", "(module (table 1 funcref) (func (export \"evaluate\") (param i64) (result i64) local.get 0))"] {
        assert!(evaluate_i64(&wat::parse_str(text).unwrap(),0).is_err());
    }
}
#[test]
fn bad_modules_and_signatures_denied() {
    assert!(evaluate_i64(b"garbage", 0).is_err());
    assert!(evaluate_i64(&vec![0; 65537], 0).is_err());
    let bytes = wat::parse_str(
        "(module (func (export \"evaluate\") (param i32) (result i32) local.get 0))",
    )
    .unwrap();
    assert!(evaluate_i64(&bytes, 0).is_err());
}
