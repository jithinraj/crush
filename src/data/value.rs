use std::cmp::Ordering;
use std::hash::Hasher;
use std::path::Path;

use chrono::{DateTime, Local};
use regex::Regex;

use crate::{
    closure::Closure,
    env::get_cwd,
    data::rows::Rows,
    errors::{error, JobError, to_job_error},
    glob::Glob,
};
use crate::data::{List, Command, Stream, ValueType, Dict, ColumnType, value_type_parser, BinaryReader};
use crate::errors::JobResult;
use std::time::Duration;
use crate::format::duration_format;
use crate::env::Env;
use crate::data::row::Struct;
use crate::stream::streams;

#[derive(Debug)]
pub enum Value {
    Text(Box<str>),
    Integer(i128),
    Time(DateTime<Local>),
    Duration(Duration),
    Field(Vec<Box<str>>),
    Glob(Glob),
    Regex(Box<str>, Regex),
    Op(Box<str>),
    Command(Command),
    Closure(Closure),
    Stream(Stream),
    File(Box<Path>),
    Rows(Rows),
    Struct(Struct),
    List(List),
    Dict(Dict),
    Env(Env),
    Bool(bool),
    Float(f64),
    Empty(),
    BinaryReader(Box<dyn BinaryReader>),
    Type(ValueType),
}

impl Value {
    pub fn to_string(&self) -> String {
        return match self {
            Value::Text(val) => val.to_string(),
            Value::Integer(val) => val.to_string(),
            Value::Time(val) => val.format("%Y-%m-%d %H:%M:%S %z").to_string(),
            Value::Field(val) => format!(r"%{}", val.join(".")),
            Value::Glob(val) => format!("*{{{}}}", val.to_string()),
            Value::Regex(val, _) => format!("r{{{}}}", val),
            Value::Op(val) => val.to_string(),
            Value::Command(_) => "Command".to_string(),
            Value::File(val) => val.to_str().unwrap_or("<Broken file>").to_string(),
            Value::Rows(_) => "<Rows>".to_string(),
            Value::Struct(row) => row.to_string(),
            Value::Closure(c) => c.to_string(),
            Value::Stream(_) => "<Output>".to_string(),
            Value::List(l) => l.to_string(),
            Value::Duration(d) => duration_format(d),
            Value::Env(env) => env.to_string(),
            Value::Bool(v) => (if *v { "true" } else { "false" }).to_string(),
            Value::Dict(d) => d.to_string(),
            Value::Float(f) => f.to_string(),
            Value::Empty() => "<empty>".to_string(),
            Value::BinaryReader(_) => "<binary>".to_string(),
            Value::Type(t) => t.to_string(),
        };
    }

    pub fn alignment(&self) -> Alignment {
        return match self {
            Value::Time(_) | Value::Duration(_) | Value::Integer(_) => Alignment::Right,
            _ => Alignment::Left,
        };
    }

    pub fn empty_stream() -> Value {
        let (_s, r) = streams(vec![]);
        Value::Stream(Stream {stream: r})
    }

    pub fn text(s: &str) -> Value {
        Value::Text(Box::from(s))
    }

    pub fn op(s: &str) -> Value {
        Value::Op(Box::from(s))
    }

    pub fn value_type(&self) -> ValueType {
        return match self {
            Value::Text(_) => ValueType::Text,
            Value::Integer(_) => ValueType::Integer,
            Value::Time(_) => ValueType::Time,
            Value::Field(_) => ValueType::Field,
            Value::Glob(_) => ValueType::Glob,
            Value::Regex(_, _) => ValueType::Regex,
            Value::Op(_) => ValueType::Op,
            Value::Command(_) => ValueType::Command,
            Value::File(_) => ValueType::File,
            Value::Stream(o) => ValueType::Stream(o.stream.get_type().clone()),
            Value::Rows(r) => ValueType::Rows(r.types.clone()),
            Value::Struct(r) => ValueType::Row(r.types.clone()),
            Value::Closure(_) => ValueType::Closure,
            Value::List(l) => l.list_type(),
            Value::Duration(_) => ValueType::Duration,
            Value::Env(_) => ValueType::Env,
            Value::Bool(_) => ValueType::Bool,
            Value::Dict(d) => d.dict_type(),
            Value::Float(_) => ValueType::Float,
            Value::Empty() => ValueType::Empty,
            Value::BinaryReader(_) => ValueType::Binary,
            Value::Type(_) => ValueType::Type,
        };
    }

    pub fn file_expand(&self, v: &mut Vec<Box<Path>>) -> JobResult<()> {
        match self {
            Value::Text(s) => v.push(Box::from(Path::new(s.as_ref()))),
            Value::File(p) => v.push(p.clone()),
            Value::Glob(pattern) => to_job_error(pattern.glob_files(
                &get_cwd()?, v))?,
            _ => return Err(error("Expected a file name")),
        }
        Ok(())
    }

    pub fn partial_clone(&self) -> Result<Value, JobError> {
        return match self {
            Value::Text(v) => Ok(Value::Text(v.clone())),
            Value::Integer(v) => Ok(Value::Integer(v.clone())),
            Value::Time(v) => Ok(Value::Time(v.clone())),
            Value::Field(v) => Ok(Value::Field(v.clone())),
            Value::Glob(v) => Ok(Value::Glob(v.clone())),
            Value::Regex(v, r) => Ok(Value::Regex(v.clone(), r.clone())),
            Value::Op(v) => Ok(Value::Op(v.clone())),
            Value::Command(v) => Ok(Value::Command(v.clone())),
            Value::File(v) => Ok(Value::File(v.clone())),
            Value::Rows(r) => Ok(Value::Rows(r.partial_clone()?)),
            Value::Struct(r) => Ok(Value::Struct(r.partial_clone()?)),
            Value::Closure(c) => Ok(Value::Closure(c.clone())),
            Value::Stream(_) => Err(error("Invalid use of stream")),
            Value::List(l) => Ok(Value::List(l.partial_clone()?)),
            Value::Duration(d) => Ok(Value::Duration(d.clone())),
            Value::Env(e) => Ok(Value::Env(e.clone())),
            Value::Bool(v) => Ok(Value::Bool(v.clone())),
            Value::Dict(d) => Ok(Value::Dict(d.partial_clone()?)),
            Value::Float(f) => Ok(Value::Float(f.clone())),
            Value::Empty() => Ok(Value::Empty()),
            Value::BinaryReader(v) => Ok(Value::BinaryReader(v.try_clone()?)),
            Value::Type(t) => Ok(Value::Type(t.clone())),
        };
    }

    pub fn materialize(self) -> Value {
        match self {
            Value::Stream(output) => {
                let mut rows = Vec::new();
                loop {
                    match output.stream.recv() {
                        Ok(r) => rows.push(r.materialize()),
                        Err(_) => break,
                    }
                }
                Value::Rows(Rows { types: ColumnType::materialize(output.stream.get_type()), rows: rows })
            }
            Value::Rows(r) => Value::Rows(r.materialize()),
            Value::Dict(d) => Value::Dict(d.materialize()),
            Value::Struct(r) => Value::Struct(r.materialize()),
            Value::List(l) => Value::List(l.materialize()),
            _ => self,
        }
    }

    pub fn cast(self, new_type: ValueType) -> Result<Value, JobError> {
        if self.value_type() == new_type {
            return Ok(self);
        }

        /*
        This function is silly and overly large. Instead of mathcing on every source/destination pair, it should do
        two matches, one to convert any cell to a string, and one to convert a string to any cell. That would shorten
        this monstrosity to a sane size.
        */
        match (self, new_type) {
            (Value::Text(s), ValueType::File) => Ok(Value::File(Box::from(Path::new(s.as_ref())))),
            (Value::Text(s), ValueType::Glob) => Ok(Value::Glob(Glob::new(&s))),
            (Value::Text(s), ValueType::Integer) => to_job_error(s.parse::<i128>()).map(|v| Value::Integer(v)),
            (Value::Text(s), ValueType::Field) => Ok(Value::Field(vec![s])),
            (Value::Text(s), ValueType::Op) => Ok(Value::Op(s)),
            (Value::Text(s), ValueType::Regex) => to_job_error(Regex::new(s.as_ref()).map(|v| Value::Regex(s, v))),
            (Value::Text(s), ValueType::Type) => Ok(Value::Type(value_type_parser::parse(s.as_ref())?)),

            (Value::File(s), ValueType::Text) => match s.to_str() {
                Some(s) => Ok(Value::Text(Box::from(s))),
                None => Err(error("File name is not valid unicode"))
            },
            (Value::File(s), ValueType::Glob) => match s.to_str() {
                Some(s) => Ok(Value::Glob(Glob::new(s))),
                None => Err(error("File name is not valid unicode"))
            },
            (Value::File(s), ValueType::Integer) => match s.to_str() {
                Some(s) => to_job_error(s.parse::<i128>()).map(|v| Value::Integer(v)),
                None => Err(error("File name is not valid unicode"))
            },
            (Value::File(s), ValueType::Op) => match s.to_str() {
                Some(s) => Ok(Value::Op(Box::from(s))),
                None => Err(error("File name is not valid unicode"))
            },
            (Value::File(s), ValueType::Regex) => match s.to_str() {
                Some(s) => to_job_error(Regex::new(s.as_ref()).map(|v| Value::Regex(Box::from(s), v))),
                None => Err(error("File name is not valid unicode"))
            },

            (Value::Glob(s), ValueType::Text) => Ok(Value::Text(s.to_string().clone().into_boxed_str())),
            (Value::Glob(s), ValueType::File) => Ok(Value::File(Box::from(Path::new(s.to_string().as_str())))),
            (Value::Glob(s), ValueType::Integer) => to_job_error(s.to_string().parse::<i128>()).map(|v| Value::Integer(v)),
            (Value::Glob(s), ValueType::Op) => Ok(Value::op(s.to_string().as_str())),
            (Value::Glob(g), ValueType::Regex) => {
                let s = g.to_string().as_str();
                to_job_error(Regex::new(s).map(|v| Value::Regex(Box::from(s), v)))
            }
            /*
                        (Cell::Field(s), CellType::File) => Ok(Cell::File(Box::from(Path::new(s.as_ref())))),
                        (Cell::Field(s), CellType::Glob) => Ok(Cell::Glob(Glob::new(&s))),
                        (Cell::Field(s), CellType::Integer) => to_job_error(s.parse::<i128>()).map(|v| Cell::Integer(v)),
                        (Cell::Field(s), CellType::Text) => Ok(Cell::Text(s)),
                        (Cell::Field(s), CellType::Op) => Ok(Cell::Op(s)),
                        (Cell::Field(s), CellType::Regex) => to_job_error(Regex::new(s.as_ref()).map(|v| Cell::Regex(s, v))),
            */
            (Value::Regex(s, _), ValueType::File) => Ok(Value::File(Box::from(Path::new(s.as_ref())))),
            (Value::Regex(s, _), ValueType::Glob) => Ok(Value::Glob(Glob::new(&s))),
            (Value::Regex(s, _), ValueType::Integer) => to_job_error(s.parse::<i128>()).map(|v| Value::Integer(v)),
            (Value::Regex(s, _), ValueType::Text) => Ok(Value::Text(s)),
            (Value::Regex(s, _), ValueType::Op) => Ok(Value::Op(s)),

            (Value::Integer(i), ValueType::Text) => Ok(Value::Text(i.to_string().into_boxed_str())),
            (Value::Integer(i), ValueType::File) => Ok(Value::File(Box::from(Path::new(i.to_string().as_str())))),
            (Value::Integer(i), ValueType::Glob) => Ok(Value::Glob(Glob::new(i.to_string().as_str()))),
            (Value::Integer(i), ValueType::Field) => Ok(Value::Field(vec![i.to_string().into_boxed_str()])),
            (Value::Integer(i), ValueType::Op) => Ok(Value::Op(i.to_string().into_boxed_str())),
            (Value::Integer(i), ValueType::Regex) => {
                let s = i.to_string();
                to_job_error(Regex::new(s.as_str()).map(|v| Value::Regex(s.into_boxed_str(), v)))
            }

            (Value::Type(s), ValueType::Text) => Ok(Value::Text(Box::from(s.to_string()))),

            _ => Err(error("Unimplemented conversion")),
        }
    }
}

impl std::hash::Hash for Value {
    fn hash<H: Hasher>(&self, state: &mut H) {
        if !self.value_type().is_hashable() {
            panic!("Can't hash mutable cell types!");
        }
        match self {
            Value::Text(v) => v.hash(state),
            Value::Integer(v) => v.hash(state),
            Value::Time(v) => v.hash(state),
            Value::Field(v) => v.hash(state),
            Value::Glob(v) => v.hash(state),
            Value::Regex(v, _) => v.hash(state),
            Value::Op(v) => v.hash(state),
            Value::Command(_) => {}
            Value::File(v) => v.hash(state),
            Value::Duration(d) => d.hash(state),
            Value::Bool(v) => v.hash(state),

            Value::Env(_) | Value::Dict(_) | Value::Rows(_) | Value::Closure(_) |
            Value::List(_) | Value::Stream(_) | Value::Struct(_) | Value::Float(_)
            | Value::BinaryReader(_) => panic!("Can't hash output"),
            Value::Empty() => {}
            Value::Type(v) => v.to_string().hash(state),
        }
    }
}

fn file_result_compare(f1: &Path, f2: &Path) -> bool {
    match (f1.canonicalize(), f2.canonicalize()) {
        (Ok(p1), Ok(p2)) => p1 == p2,
        _ => false,
    }
}

impl std::cmp::PartialEq for Value {
    fn eq(&self, other: &Value) -> bool {
        return match (self, other) {
            (Value::Text(val1), Value::Text(val2)) => val1 == val2,
            (Value::Glob(glb), Value::Text(val)) => glb.matches(val),
            (Value::Text(val), Value::Glob(glb)) => glb.matches(val),
            (Value::Integer(val1), Value::Integer(val2)) => val1 == val2,
            (Value::Time(val1), Value::Time(val2)) => val1 == val2,
            (Value::Duration(val1), Value::Duration(val2)) => val1 == val2,
            (Value::Field(val1), Value::Field(val2)) => val1 == val2,
            (Value::Glob(val1), Value::Glob(val2)) => val1 == val2,
            (Value::Regex(val1, _), Value::Regex(val2, _)) => val1 == val2,
            (Value::Op(val1), Value::Op(val2)) => val1 == val2,
            (Value::Command(val1), Value::Command(val2)) => val1 == val2,
            (Value::List(val1), Value::List(val2)) => val1 == val2,
            (Value::Rows(val1), Value::Rows(val2)) => match val1.partial_cmp(val2) {
                None => false,
                Some(o) => o == Ordering::Equal,
            },
            (Value::Struct(val1), Value::Struct(val2)) => match val1.partial_cmp(val2) {
                None => false,
                Some(o) => o == Ordering::Equal,
            },
            (Value::File(val1), Value::File(val2)) => file_result_compare(val1.as_ref(), val2.as_ref()),
            (Value::Text(val1), Value::File(val2)) => file_result_compare(&Path::new(&val1.to_string()), val2.as_ref()),
            (Value::File(val1), Value::Text(val2)) => file_result_compare(&Path::new(&val2.to_string()), val1.as_ref()),
            (Value::Bool(val1), Value::Bool(val2)) => val1 == val2,
            _ => false,
        };
    }
}

pub enum Alignment {
    Left,
    Right,
}

impl std::cmp::PartialOrd for Value {
    fn partial_cmp(&self, other: &Value) -> Option<Ordering> {
        let t1 = self.value_type();
        let t2 = other.value_type();
        if t1 != t2 {
            return Some(t1.cmp(&t2));
        }
        return match (self, other) {
            (Value::Text(val1), Value::Text(val2)) => Some(val1.cmp(val2)),
            (Value::Field(val1), Value::Field(val2)) => Some(val1.cmp(val2)),
            (Value::Glob(val1), Value::Glob(val2)) => Some(val1.cmp(val2)),
            (Value::Regex(val1, _), Value::Regex(val2, _)) => Some(val1.cmp(val2)),
            (Value::Integer(val1), Value::Integer(val2)) => Some(val1.cmp(val2)),
            (Value::Time(val1), Value::Time(val2)) => Some(val1.cmp(val2)),
            (Value::Op(val1), Value::Op(val2)) => Some(val1.cmp(val2)),
            (Value::File(val1), Value::File(val2)) => Some(val1.cmp(val2)),
            (Value::Duration(val1), Value::Duration(val2)) => Some(val1.cmp(val2)),
            (Value::Command(_), Value::Command(_)) => None,
            (Value::Closure(_), _) => None,
            (Value::Stream(_), _) => None,
            (Value::Rows(val1), Value::Rows(val2)) => val1.partial_cmp(val2),
            (Value::Struct(val1), Value::Struct(val2)) => val1.partial_cmp(val2),
            (Value::List(val1), Value::List(val2)) => val1.partial_cmp(val2),
            (Value::Bool(val1), Value::Bool(val2)) => Some(val1.cmp(val2)),
            _ => None,
        };
    }
}

impl std::cmp::Eq for Value {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_casts() {
        assert_eq!(Value::Text(Box::from("112432")).cast(ValueType::Integer).is_err(), false);
        assert_eq!(Value::text("1d").cast(ValueType::Integer).is_err(), true);
        assert_eq!(Value::text("1d").cast(ValueType::Glob).is_err(), false);
        assert_eq!(Value::text("1d").cast(ValueType::File).is_err(), false);
        assert_eq!(Value::text("1d").cast(ValueType::Time).is_err(), true);
        assert_eq!(Value::text("fad").cast(ValueType::Field).is_err(), false);
        assert_eq!(Value::text("fad").cast(ValueType::Op).is_err(), false);
    }

    #[test]
    fn test_duration_format() {
        assert_eq!(duration_format(&Duration::from_micros(0)), "0".to_string());
        assert_eq!(duration_format(&Duration::from_micros(1)), "0.000001".to_string());
        assert_eq!(duration_format(&Duration::from_micros(100)), "0.0001".to_string());
        assert_eq!(duration_format(&Duration::from_millis(1)), "0.001".to_string());
        assert_eq!(duration_format(&Duration::from_millis(1000)), "1".to_string());
        assert_eq!(duration_format(&Duration::from_millis(1000 * 61)), "1:01".to_string());
        assert_eq!(duration_format(&Duration::from_millis(1000 * 3601)), "1:00:01".to_string());
        assert_eq!(duration_format(&Duration::from_millis(1000 * (3600 * 24 * 3 + 1))), "3d0:00:01".to_string());
        assert_eq!(duration_format(&Duration::from_millis(1000 * (3600 * 24 * 365 * 10 + 1))), "10y0d0:00:01".to_string());
        assert_eq!(duration_format(&Duration::from_millis(1000 * (3600 * 24 * 365 * 10 + 1) + 1)), "10y0d0:00:01".to_string());
    }
}
