use crate::lang::execution_context::{ExecutionContext, ArgumentVector};
use std::collections::HashSet;
use crate::lang::argument::Argument;
use crate::lang::table::Row;
use crate::lang::{value::Value, table::ColumnType};
use crate::lang::errors::{CrushResult, error};
use crate::lang::stream::{CrushStream, OutputStream};
use crate::lang::table::ColumnVec;
use crate::lang::printer::Printer;

fn parse(input_type: &[ColumnType], mut arguments: Vec<Argument>) -> CrushResult<Option<usize>> {
    arguments.check_len_range(0, 1)?;
    if let Some(f) = arguments.optional_field(0)? {
        Ok(Some(input_type.find(&f)?))
    } else {
        Ok(None)
    }
}

fn run(
    idx: Option<usize>,
    input: &mut dyn CrushStream,
    output: OutputStream,
    printer: &Printer,
) -> CrushResult<()> {
    match idx {
        None => {
            let mut seen: HashSet<Row> = HashSet::new();
            while let Ok(row) = input.read() {
                if !seen.contains(&row) {
                    seen.insert(row.clone());
                    printer.handle_error(output.send(row));
                }
            }
        }
        Some(idx) => {
            let mut seen: HashSet<Value> = HashSet::new();
            while let Ok(row) = input.read() {
                if !seen.contains(&row.cells()[idx]) {
                    seen.insert(row.cells()[idx].clone());
                    printer.handle_error(output.send(row));
                }
            }
        }
    }
    Ok(())
}

pub fn uniq(context: ExecutionContext) -> CrushResult<()> {
    match context.input.recv()?.stream() {
        Some(mut input) => {
            let idx = parse(input.types(), context.arguments)?;
            let output = context.output.initialize(input.types().to_vec())?;
            run(idx, input.as_mut(), output, &context.printer)
        }
        _ => error("Expected io to be a stream"),
    }
}
