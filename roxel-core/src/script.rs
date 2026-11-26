//! JavaScript plugin host on QuickJS. A plugin is an ES module whose default
//! export is a manifest; `docs/src/roxel.d.ts` is the contract for everything
//! a script can see. Scripts never write the grid: `set`/`clear` stage edits
//! and the caller applies them, so all writes still go through `History`.

use crate::grid::{Color8, VoxelGrid};
use crate::history::History;
use glam::IVec3;
use rquickjs::allocator::{Allocator, RustAllocator};
use rquickjs::{
    Array, CatchResultExt, CaughtError, Context, Ctx, Exception, Function, Object, Runtime, Value,
};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

mod params;
pub use params::{ParamKind, ParamSpec, resolve_params};

pub const API_VERSION: i32 = 1;

/// Distinct cells one run may stage. Each staged cell costs ~40 bytes on the
/// Rust side, outside the QuickJS memory limit.
pub const MAX_EDITS: usize = 1 << 22;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edit {
    pub pos: IVec3,
    pub color: Option<Color8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub time: Duration,
    /// QuickJS heap cap in bytes.
    pub memory: usize,
    pub edits: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            time: Duration::from_secs(5),
            memory: 256 << 20,
            edits: MAX_EDITS,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Param {
    Number(f64),
    Bool(bool),
    Text(String),
    Color(Color8),
}

/// Inclusive box; `mask`, when present, narrows it to those cells.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    pub min: IVec3,
    pub max: IVec3,
    pub mask: Option<HashSet<IVec3>>,
}

impl Selection {
    fn contains(&self, p: IVec3) -> bool {
        match &self.mask {
            Some(mask) => mask.contains(&p),
            None => p.cmpge(self.min).all() && p.cmple(self.max).all(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScriptCtx {
    /// Id of the manifest command to run.
    pub command: String,
    pub selection: Option<Selection>,
    pub color: Color8,
    pub palette: Vec<Color8>,
    pub params: Vec<(String, Param)>,
    pub limits: Limits,
}

impl Default for ScriptCtx {
    fn default() -> Self {
        Self {
            command: String::new(),
            selection: None,
            color: [255, 255, 255, 255],
            palette: Vec::new(),
            params: Vec::new(),
            limits: Limits::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Manifest {
    pub name: String,
    pub commands: Vec<Command>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Command {
    pub id: String,
    pub label: String,
    pub params: Vec<ParamSpec>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScriptError {
    /// The module didn't parse, its top level threw, or the manifest is malformed.
    Load(String),
    /// The manifest's `apiVersion`, as written in the source.
    UnsupportedApiVersion(String),
    UnknownCommand(String),
    /// The command threw.
    Run(String),
    TimeLimit(Duration),
    MemoryLimit(usize),
    EditLimit(usize),
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Load(msg) | Self::Run(msg) => f.write_str(msg),
            Self::UnsupportedApiVersion(v) => write!(
                f,
                "plugin needs apiVersion {v}, but this version of Roxel supports apiVersion {API_VERSION}"
            ),
            Self::UnknownCommand(id) => write!(f, "plugin has no command \"{id}\""),
            Self::TimeLimit(d) => write!(f, "stopped after the {} s time limit", d.as_secs_f32()),
            Self::MemoryLimit(bytes) => {
                write!(f, "stopped at the {} MB memory limit", bytes >> 20)
            }
            Self::EditLimit(n) => write!(f, "stopped at the limit of {n} changed voxels"),
        }
    }
}

impl std::error::Error for ScriptError {}

/// Reads the manifest without running any command.
pub fn load(file: &str, source: &str, limits: &Limits) -> Result<Manifest, ScriptError> {
    let host = Host::new(limits)?;
    let result = host.context.with(|ctx| {
        let (manifest, _) = host.check(&ctx, file, eval_manifest(&ctx, file, source))?;
        Ok(manifest)
    });
    host.caught_limit()?;
    result
}

/// Runs one command and returns the cells it changed, in the order first
/// staged. Reads see `grid` as it was before the run. The grid is moved out
/// for the run and back before returning, unchanged.
pub fn run(
    file: &str,
    source: &str,
    grid: &mut VoxelGrid,
    cx: &ScriptCtx,
) -> Result<Vec<Edit>, ScriptError> {
    let loan = Loan::new(grid);
    let staged = Rc::new(RefCell::new(Staged::new(cx.limits.edits)));
    let host = Host::new(&cx.limits)?;
    let grid = loan.grid();
    let result = host.context.with(|ctx| {
        let (manifest, plugin) = host.check(&ctx, file, eval_manifest(&ctx, file, source))?;
        let Some(command) = manifest.commands.iter().find(|c| c.id == cx.command) else {
            return Err(ScriptError::UnknownCommand(cx.command.clone()));
        };
        let params = resolve_params(&command.params, &cx.params);
        let run_fn = command_run(&plugin, &cx.command).catch(&ctx);
        let run_fn = host.check(&ctx, file, run_fn.map_err(Into::into))?;
        let out = (|| {
            let js_ctx = build_ctx(&ctx, &grid, &staged, cx, &params)?;
            run_fn.call::<_, Value>((js_ctx,))
        })()
        .catch(&ctx);
        let out = host
            .check(&ctx, file, out.map_err(Into::into))
            .map_err(load_to_run)?;
        if out.is_promise() {
            return Err(ScriptError::Run(format!(
                "{file}: command \"{}\" returned a Promise; run() must be synchronous",
                cx.command
            )));
        }
        Ok(())
    });
    if staged.borrow().overflowed {
        return Err(ScriptError::EditLimit(cx.limits.edits));
    }
    host.caught_limit()?;
    result?;
    let staged = staged.take();
    Ok(staged
        .edits
        .into_iter()
        .filter(|e| grid.get(e.pos) != e.color)
        .collect())
}

/// Writes a run's edits as one stroke, so one Undo reverts the whole run.
pub fn apply(edits: &[Edit], history: &mut History, grid: &mut VoxelGrid) {
    history.begin();
    for e in edits {
        history.record(grid, e.pos, e.color);
    }
    history.end();
}

/// Moves the grid into an `Rc` the JS bindings can hold, and back on drop.
/// The runtime, and every binding in it, drops before the loan does.
struct Loan<'a> {
    slot: &'a mut VoxelGrid,
    grid: Option<Rc<VoxelGrid>>,
}

impl<'a> Loan<'a> {
    fn new(slot: &'a mut VoxelGrid) -> Self {
        let grid = Some(Rc::new(std::mem::take(slot)));
        Self { slot, grid }
    }

    fn grid(&self) -> Rc<VoxelGrid> {
        self.grid.clone().expect("loan is live")
    }
}

impl Drop for Loan<'_> {
    fn drop(&mut self) {
        let grid = self.grid.take().and_then(Rc::into_inner);
        debug_assert!(grid.is_some(), "a binding outlived the run");
        if let Some(grid) = grid {
            *self.slot = grid;
        }
    }
}

struct Staged {
    index: HashMap<IVec3, usize>,
    edits: Vec<Edit>,
    limit: usize,
    overflowed: bool,
}

impl Staged {
    fn new(limit: usize) -> Self {
        Self {
            index: HashMap::new(),
            edits: Vec::new(),
            limit,
            overflowed: false,
        }
    }

    /// `false` when the cap is hit. Below-floor writes are dropped, like `VoxelGrid::set`.
    fn stage(&mut self, pos: IVec3, color: Option<Color8>) -> bool {
        if pos.y < 0 {
            return true;
        }
        if let Some(&i) = self.index.get(&pos) {
            self.edits[i].color = color;
            return true;
        }
        if self.edits.len() >= self.limit {
            self.overflowed = true;
            return false;
        }
        self.index.insert(pos, self.edits.len());
        self.edits.push(Edit { pos, color });
        true
    }
}

impl Default for Staged {
    fn default() -> Self {
        Self::new(0)
    }
}

enum Failure<'js> {
    Js(CaughtError<'js>),
    Script(ScriptError),
}

impl<'js> From<CaughtError<'js>> for Failure<'js> {
    fn from(e: CaughtError<'js>) -> Self {
        Self::Js(e)
    }
}

impl From<ScriptError> for Failure<'_> {
    fn from(e: ScriptError) -> Self {
        Self::Script(e)
    }
}

struct Host {
    // Field order is drop order: the context must go before its runtime.
    context: Context,
    _runtime: Runtime,
    interrupted: Arc<AtomicBool>,
    out_of_memory: Arc<AtomicBool>,
    limits: Limits,
}

impl Host {
    fn new(limits: &Limits) -> Result<Self, ScriptError> {
        let engine = |e: rquickjs::Error| ScriptError::Load(format!("couldn't start QuickJS: {e}"));
        let out_of_memory = Arc::new(AtomicBool::new(false));
        let runtime = Runtime::new_with_alloc(CappedAllocator {
            used: 0,
            limit: limits.memory,
            hit: out_of_memory.clone(),
        })
        .map_err(engine)?;
        let interrupted = Arc::new(AtomicBool::new(false));
        let deadline = Instant::now() + limits.time;
        let flag = interrupted.clone();
        runtime.set_interrupt_handler(Some(Box::new(move || {
            let over = Instant::now() >= deadline;
            if over {
                flag.store(true, Ordering::Relaxed);
            }
            over
        })));
        let context = Context::full(&runtime).map_err(engine)?;
        Ok(Self {
            context,
            _runtime: runtime,
            interrupted,
            out_of_memory,
            limits: *limits,
        })
    }

    /// Turns a failure into a `ScriptError::Load`, or a limit error when a
    /// limit caused it.
    fn check<'js, T>(
        &self,
        ctx: &Ctx<'js>,
        file: &str,
        result: Result<T, Failure<'js>>,
    ) -> Result<T, ScriptError> {
        let caught = match result {
            Ok(v) => return Ok(v),
            Err(Failure::Script(e)) => return Err(e),
            Err(Failure::Js(e)) => e,
        };
        if self.interrupted.load(Ordering::Relaxed) {
            return Err(ScriptError::TimeLimit(self.limits.time));
        }
        if self.out_of_memory.load(Ordering::Relaxed) {
            return Err(ScriptError::MemoryLimit(self.limits.memory));
        }
        Err(ScriptError::Load(describe(ctx, file, caught)))
    }

    /// A script can catch the out-of-memory error and return normally.
    fn caught_limit(&self) -> Result<(), ScriptError> {
        if self.out_of_memory.load(Ordering::Relaxed) {
            return Err(ScriptError::MemoryLimit(self.limits.memory));
        }
        Ok(())
    }
}

/// QuickJS's own `set_memory_limit` can't say whether it fired: when the heap
/// is too full to allocate its "out of memory" error, the script sees `null`
/// thrown. This allocator records every refusal.
struct CappedAllocator {
    used: usize,
    limit: usize,
    hit: Arc<AtomicBool>,
}

impl CappedAllocator {
    fn admit(&mut self, freed: usize, wanted: usize) -> bool {
        let ok = self.used - freed + wanted <= self.limit;
        if !ok {
            self.hit.store(true, Ordering::Relaxed);
        }
        ok
    }
}

// SAFETY: every pointer comes from and goes back to `RustAllocator`; this only
// adds bookkeeping and refuses requests over the limit by returning null.
unsafe impl Allocator for CappedAllocator {
    fn alloc(&mut self, size: usize) -> *mut u8 {
        if !self.admit(0, size) {
            return std::ptr::null_mut();
        }
        let ptr = RustAllocator.alloc(size);
        if !ptr.is_null() {
            self.used += unsafe { RustAllocator::usable_size(ptr) };
        }
        ptr
    }

    fn calloc(&mut self, count: usize, size: usize) -> *mut u8 {
        if !self.admit(0, count.saturating_mul(size)) {
            return std::ptr::null_mut();
        }
        let ptr = RustAllocator.calloc(count, size);
        if !ptr.is_null() {
            self.used += unsafe { RustAllocator::usable_size(ptr) };
        }
        ptr
    }

    unsafe fn dealloc(&mut self, ptr: *mut u8) {
        unsafe {
            self.used -= RustAllocator::usable_size(ptr);
            RustAllocator.dealloc(ptr);
        }
    }

    unsafe fn realloc(&mut self, ptr: *mut u8, new_size: usize) -> *mut u8 {
        if ptr.is_null() {
            return self.alloc(new_size);
        }
        unsafe {
            let old = RustAllocator::usable_size(ptr);
            if !self.admit(old, new_size) {
                return std::ptr::null_mut();
            }
            let new = RustAllocator.realloc(ptr, new_size);
            if !new.is_null() {
                self.used = self.used - old + RustAllocator::usable_size(new);
            }
            new
        }
    }

    unsafe fn usable_size(ptr: *mut u8) -> usize {
        unsafe { RustAllocator::usable_size(ptr) }
    }
}

fn load_to_run(e: ScriptError) -> ScriptError {
    match e {
        ScriptError::Load(msg) => ScriptError::Run(msg),
        other => other,
    }
}

/// `file:line: Name: message`, with the line taken from the innermost frame in `file`.
fn describe<'js>(ctx: &Ctx<'js>, file: &str, caught: CaughtError<'js>) -> String {
    match caught {
        CaughtError::Exception(ex) => {
            let name: String = ex.get("name").unwrap_or_else(|_| "Error".into());
            let message = ex.message().unwrap_or_default();
            let line = ex.stack().and_then(|s| line_in(&s, file));
            match line {
                Some(line) => format!("{file}:{line}: {name}: {message}"),
                None => format!("{file}: {name}: {message}"),
            }
        }
        CaughtError::Value(v) => {
            let text = ctx
                .json_stringify(v)
                .ok()
                .flatten()
                .and_then(|s| s.to_string().ok())
                .unwrap_or_else(|| "a non-Error value".into());
            format!("{file}: uncaught {text}")
        }
        CaughtError::Error(e) => format!("{file}: {e}"),
    }
}

fn line_in(stack: &str, file: &str) -> Option<u32> {
    let needle = format!("{file}:");
    let at = stack.match_indices(&needle).find_map(|(i, _)| {
        let boundary = stack[..i]
            .chars()
            .next_back()
            .is_none_or(|c| c == '(' || c == ' ');
        boundary.then_some(i + needle.len())
    })?;
    let digits: String = stack[at..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

fn eval_manifest<'js>(
    ctx: &Ctx<'js>,
    file: &str,
    source: &str,
) -> Result<(Manifest, Object<'js>), Failure<'js>> {
    let module = rquickjs::Module::declare(ctx.clone(), file, source).catch(ctx)?;
    let (module, promise) = module.eval().catch(ctx)?;
    promise.finish::<()>().catch(ctx)?;
    let exports = module.namespace().catch(ctx)?;
    let default: Value = exports.get("default").catch(ctx)?;
    let Some(plugin) = default.into_object() else {
        return Err(bad(file, "has no default export object").into());
    };
    let manifest = read_manifest(ctx, file, &plugin)?;
    Ok((manifest, plugin))
}

fn bad(file: &str, what: &str) -> ScriptError {
    ScriptError::Load(format!("{file}: manifest {what}"))
}

fn read_manifest<'js>(
    ctx: &Ctx<'js>,
    file: &str,
    plugin: &Object<'js>,
) -> Result<Manifest, Failure<'js>> {
    let version: Value = plugin.get("apiVersion").catch(ctx)?;
    if version.is_undefined() {
        return Err(bad(file, "is missing apiVersion").into());
    }
    if version.as_number() != Some(f64::from(API_VERSION)) {
        let shown = ctx
            .json_stringify(version)
            .ok()
            .flatten()
            .and_then(|s| s.to_string().ok())
            .unwrap_or_else(|| "?".into());
        return Err(ScriptError::UnsupportedApiVersion(shown).into());
    }
    let name = string_field(ctx, file, plugin, "name", "name")?;
    let commands: Value = plugin.get("commands").catch(ctx)?;
    let Some(commands) = commands.into_array() else {
        return Err(bad(file, "commands must be an array").into());
    };
    let mut out: Vec<Command> = Vec::new();
    for (i, cmd) in commands.iter::<Value>().enumerate() {
        let cmd = cmd.catch(ctx)?;
        let Some(cmd) = cmd.into_object() else {
            return Err(bad(file, &format!("commands[{i}] must be an object")).into());
        };
        let id = string_field(ctx, file, &cmd, "id", &format!("commands[{i}].id"))?;
        let label = string_field(ctx, file, &cmd, "label", &format!("commands[{i}].label"))?;
        let run: Value = cmd.get("run").catch(ctx)?;
        if !run.is_function() {
            return Err(bad(file, &format!("commands[{i}].run must be a function")).into());
        }
        if out.iter().any(|c| c.id == id) {
            return Err(bad(file, &format!("has two commands with id \"{id}\"")).into());
        }
        let params = read_params(ctx, file, &cmd, &format!("commands[{i}].params"))?;
        out.push(Command { id, label, params });
    }
    Ok(Manifest {
        name,
        commands: out,
    })
}

fn string_field<'js>(
    ctx: &Ctx<'js>,
    file: &str,
    obj: &Object<'js>,
    key: &str,
    path: &str,
) -> Result<String, Failure<'js>> {
    let v: Value = obj.get(key).catch(ctx)?;
    match v.as_string().map(|s| s.to_string()) {
        Some(Ok(s)) if !s.is_empty() => Ok(s),
        _ => Err(bad(file, &format!("{path} must be a non-empty string")).into()),
    }
}

fn read_params<'js>(
    ctx: &Ctx<'js>,
    file: &str,
    cmd: &Object<'js>,
    path: &str,
) -> Result<Vec<ParamSpec>, Failure<'js>> {
    let list: Value = cmd.get("params").catch(ctx)?;
    if list.is_undefined() {
        return Ok(Vec::new());
    }
    let Some(list) = list.into_array() else {
        return Err(bad(file, &format!("{path} must be an array")).into());
    };
    let mut out: Vec<ParamSpec> = Vec::new();
    for (j, p) in list.iter::<Value>().enumerate() {
        let at = format!("{path}[{j}]");
        let Some(p) = p.catch(ctx)?.into_object() else {
            return Err(bad(file, &format!("{at} must be an object")).into());
        };
        let id = string_field(ctx, file, &p, "id", &format!("{at}.id"))?;
        if out.iter().any(|s| s.id == id) {
            return Err(bad(file, &format!("{path} has two params with id \"{id}\"")).into());
        }
        let label = string_field(ctx, file, &p, "label", &format!("{at}.label"))?;
        let ty = string_field(ctx, file, &p, "type", &format!("{at}.type"))?;
        let kind = read_param_kind(ctx, file, &p, &at, &ty)?;
        out.push(ParamSpec { id, label, kind });
    }
    Ok(out)
}

fn read_param_kind<'js>(
    ctx: &Ctx<'js>,
    file: &str,
    p: &Object<'js>,
    at: &str,
    ty: &str,
) -> Result<ParamKind, Failure<'js>> {
    let field = |key: &str, what: &str, conv: &dyn Fn(&Value<'js>) -> bool| {
        let v: Value = p.get(key).catch(ctx)?;
        if v.is_undefined() {
            return Ok(None);
        }
        if !conv(&v) {
            return Err(Failure::from(bad(
                file,
                &format!("{at}.{key} must be {what}"),
            )));
        }
        Ok(Some(v))
    };
    let number = |v: &Value| v.as_number().is_some_and(f64::is_finite);
    let int = |v: &Value| int_value(v).is_some();
    let out_of_range = |key: &str| Failure::from(bad(file, &format!("{at}.{key} is out of range")));
    let kind = match ty {
        "number" => {
            let get = |key| -> Result<Option<f64>, Failure<'js>> {
                Ok(field(key, "a number", &number)?.and_then(|v| v.as_number()))
            };
            let (min, max) = (get("min")?, get("max")?);
            if let (Some(lo), Some(hi)) = (min, max)
                && lo > hi
            {
                return Err(out_of_range("min"));
            }
            let default = get("default")?.unwrap_or_else(|| params::zero_in_range(min, max));
            if min.is_some_and(|m| default < m) || max.is_some_and(|m| default > m) {
                return Err(out_of_range("default"));
            }
            ParamKind::Number { default, min, max }
        }
        "int" => {
            let get = |key| -> Result<Option<i32>, Failure<'js>> {
                Ok(field(key, "an integer", &int)?.and_then(|v| int_value(&v)))
            };
            let (min, max) = (get("min")?, get("max")?);
            if let (Some(lo), Some(hi)) = (min, max)
                && lo > hi
            {
                return Err(out_of_range("min"));
            }
            let default = get("default")?.unwrap_or_else(|| params::zero_in_range(min, max));
            if min.is_some_and(|m| default < m) || max.is_some_and(|m| default > m) {
                return Err(out_of_range("default"));
            }
            ParamKind::Int { default, min, max }
        }
        "bool" => ParamKind::Bool {
            default: field("default", "true or false", &|v| v.is_bool())?
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        },
        "color" => ParamKind::Color {
            default: field(
                "default",
                "[r, g, b] or [r, g, b, a] of integers 0-255",
                &|v| color_value(v).is_some(),
            )?
            .and_then(|v| color_value(&v))
            .unwrap_or([255; 4]),
        },
        "choice" => {
            let options = read_options(ctx, file, p, at)?;
            let default = match field("default", "a string", &|v| v.is_string())? {
                None => 0,
                Some(v) => {
                    let s: String = v.get().catch(ctx)?;
                    options.iter().position(|o| *o == s).ok_or_else(|| {
                        bad(file, &format!("{at}.default must be one of its options"))
                    })?
                }
            };
            ParamKind::Choice { options, default }
        }
        other => {
            return Err(bad(
                file,
                &format!(
                    "{at}.type must be \"number\", \"int\", \"bool\", \"color\" or \"choice\", not \"{other}\""
                ),
            )
            .into());
        }
    };
    Ok(kind)
}

fn read_options<'js>(
    ctx: &Ctx<'js>,
    file: &str,
    p: &Object<'js>,
    at: &str,
) -> Result<Vec<String>, Failure<'js>> {
    let err = || {
        Failure::from(bad(
            file,
            &format!("{at}.options must be an array of strings"),
        ))
    };
    let list: Value = p.get("options").catch(ctx)?;
    let Some(list) = list.into_array() else {
        return Err(err());
    };
    let mut out: Vec<String> = Vec::new();
    for v in list.iter::<Value>() {
        let v = v.catch(ctx)?;
        let s = match v.as_string().map(|s| s.to_string()) {
            Some(Ok(s)) if !s.is_empty() => s,
            _ => return Err(err()),
        };
        if out.contains(&s) {
            return Err(bad(file, &format!("{at}.options has \"{s}\" twice")).into());
        }
        out.push(s);
    }
    if out.is_empty() {
        return Err(bad(file, &format!("{at}.options must not be empty")).into());
    }
    Ok(out)
}

fn int_value(v: &Value) -> Option<i32> {
    if let Some(i) = v.as_int() {
        return Some(i);
    }
    v.as_float()
        .filter(|f| f.fract() == 0.0 && *f >= f64::from(i32::MIN) && *f <= f64::from(i32::MAX))
        .map(|f| f as i32)
}

fn command_run<'js>(plugin: &Object<'js>, id: &str) -> rquickjs::Result<Function<'js>> {
    let commands: Array = plugin.get("commands")?;
    for cmd in commands.iter::<Object>() {
        let cmd = cmd?;
        if cmd.get::<_, String>("id")? == id {
            return cmd.get("run");
        }
    }
    Err(rquickjs::Error::Unknown)
}

fn build_ctx<'js>(
    ctx: &Ctx<'js>,
    grid: &Rc<VoxelGrid>,
    staged: &Rc<RefCell<Staged>>,
    cx: &ScriptCtx,
    params: &[(String, Param)],
) -> rquickjs::Result<Object<'js>> {
    let out = Object::new(ctx.clone())?;
    out.set("grid", grid_object(ctx, grid, staged)?)?;
    let selection = match &cx.selection {
        Some(sel) => selection_object(ctx, sel)?.into_value(),
        None => Value::new_null(ctx.clone()),
    };
    out.set("selection", selection)?;
    out.set("color", color_js(ctx, Some(cx.color))?)?;
    let palette = Array::new(ctx.clone())?;
    for (i, c) in cx.palette.iter().enumerate() {
        palette.set(i, color_js(ctx, Some(*c))?)?;
    }
    out.set("palette", palette)?;
    let js_params = Object::new(ctx.clone())?;
    for (key, value) in params {
        let v = match value {
            Param::Number(n) => Value::new_number(ctx.clone(), *n),
            Param::Bool(b) => Value::new_bool(ctx.clone(), *b),
            Param::Text(s) => rquickjs::String::from_str(ctx.clone(), s)?.into_value(),
            Param::Color(c) => color_js(ctx, Some(*c))?,
        };
        js_params.set(key.as_str(), v)?;
    }
    out.set("params", js_params)?;
    Ok(out)
}

fn grid_object<'js>(
    ctx: &Ctx<'js>,
    grid: &Rc<VoxelGrid>,
    staged: &Rc<RefCell<Staged>>,
) -> rquickjs::Result<Object<'js>> {
    let obj = Object::new(ctx.clone())?;

    let g = grid.clone();
    obj.set(
        "get",
        Function::new(
            ctx.clone(),
            move |ctx: Ctx<'js>, x: Value<'js>, y: Value<'js>, z: Value<'js>| {
                let p = point(&ctx, "grid.get", &x, &y, &z)?;
                color_js(&ctx, g.get(p))
            },
        )?,
    )?;

    let s = staged.clone();
    obj.set(
        "set",
        Function::new(
            ctx.clone(),
            move |ctx: Ctx<'js>, x: Value<'js>, y: Value<'js>, z: Value<'js>, c: Value<'js>| {
                let p = point(&ctx, "grid.set", &x, &y, &z)?;
                let c = color_arg(&ctx, "grid.set", &c)?;
                stage(&ctx, &s, p, Some(c))
            },
        )?,
    )?;

    let s = staged.clone();
    obj.set(
        "clear",
        Function::new(
            ctx.clone(),
            move |ctx: Ctx<'js>, x: Value<'js>, y: Value<'js>, z: Value<'js>| {
                let p = point(&ctx, "grid.clear", &x, &y, &z)?;
                stage(&ctx, &s, p, None)
            },
        )?,
    )?;

    let g = grid.clone();
    obj.set(
        "bounds",
        Function::new(ctx.clone(), move |ctx: Ctx<'js>| match g.bounding_box() {
            Some((min, max)) => Ok(bounds_js(&ctx, min, max)?.into_value()),
            None => Ok::<_, rquickjs::Error>(Value::new_null(ctx)),
        })?,
    )?;

    let g = grid.clone();
    obj.set(
        "forEach",
        Function::new(ctx.clone(), move |ctx: Ctx<'js>, f: Function<'js>| {
            for (p, c) in g.iter_occupied() {
                f.call::<_, ()>((p.x, p.y, p.z, color_js(&ctx, Some(c))?))?;
            }
            Ok::<_, rquickjs::Error>(())
        })?,
    )?;

    Ok(obj)
}

fn selection_object<'js>(ctx: &Ctx<'js>, sel: &Selection) -> rquickjs::Result<Object<'js>> {
    let obj = Object::new(ctx.clone())?;
    obj.set("bounds", bounds_js(ctx, sel.min, sel.max)?)?;
    let sel = Rc::new(sel.clone());

    let s = sel.clone();
    obj.set(
        "has",
        Function::new(
            ctx.clone(),
            move |ctx: Ctx<'js>, x: Value<'js>, y: Value<'js>, z: Value<'js>| {
                Ok::<_, rquickjs::Error>(s.contains(point(&ctx, "selection.has", &x, &y, &z)?))
            },
        )?,
    )?;

    obj.set(
        "forEach",
        Function::new(ctx.clone(), move |f: Function<'js>| {
            let visit = |p: IVec3| f.call::<_, ()>((p.x, p.y, p.z));
            match &sel.mask {
                Some(mask) => mask.iter().try_for_each(|&p| visit(p)),
                None => {
                    for x in sel.min.x..=sel.max.x {
                        for y in sel.min.y..=sel.max.y {
                            for z in sel.min.z..=sel.max.z {
                                visit(IVec3::new(x, y, z))?;
                            }
                        }
                    }
                    Ok(())
                }
            }
        })?,
    )?;

    Ok(obj)
}

fn stage<'js>(
    ctx: &Ctx<'js>,
    staged: &RefCell<Staged>,
    p: IVec3,
    c: Option<Color8>,
) -> rquickjs::Result<()> {
    if staged.borrow_mut().stage(p, c) {
        Ok(())
    } else {
        Err(Exception::throw_range(ctx, "too many changed voxels"))
    }
}

fn point<'js>(
    ctx: &Ctx<'js>,
    what: &str,
    x: &Value<'js>,
    y: &Value<'js>,
    z: &Value<'js>,
) -> rquickjs::Result<IVec3> {
    Ok(IVec3::new(
        int(ctx, what, "x", x)?,
        int(ctx, what, "y", y)?,
        int(ctx, what, "z", z)?,
    ))
}

fn int<'js>(ctx: &Ctx<'js>, what: &str, axis: &str, v: &Value<'js>) -> rquickjs::Result<i32> {
    int_value(v)
        .ok_or_else(|| Exception::throw_type(ctx, &format!("{what}: {axis} must be an integer")))
}

fn color_arg<'js>(ctx: &Ctx<'js>, what: &str, v: &Value<'js>) -> rquickjs::Result<Color8> {
    color_value(v).ok_or_else(|| {
        Exception::throw_type(
            ctx,
            &format!("{what}: color must be [r, g, b] or [r, g, b, a] of integers 0-255"),
        )
    })
}

fn color_value(v: &Value) -> Option<Color8> {
    let arr = v.as_array()?;
    let len = arr.len();
    if len != 3 && len != 4 {
        return None;
    }
    let mut out = [255u8; 4];
    for (i, slot) in out.iter_mut().enumerate().take(len) {
        let n: Value = arr.get(i).ok()?;
        *slot = u8::try_from(n.as_int()?).ok()?;
    }
    Some(out)
}

fn color_js<'js>(ctx: &Ctx<'js>, c: Option<Color8>) -> rquickjs::Result<Value<'js>> {
    let Some(c) = c else {
        return Ok(Value::new_null(ctx.clone()));
    };
    let arr = Array::new(ctx.clone())?;
    for (i, v) in c.iter().enumerate() {
        arr.set(i, *v)?;
    }
    Ok(arr.into_value())
}

fn bounds_js<'js>(ctx: &Ctx<'js>, min: IVec3, max: IVec3) -> rquickjs::Result<Object<'js>> {
    let obj = Object::new(ctx.clone())?;
    obj.set("min", vec![min.x, min.y, min.z])?;
    obj.set("max", vec![max.x, max.y, max.z])?;
    Ok(obj)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: Color8 = [255, 0, 0, 255];

    fn ctx(command: &str) -> ScriptCtx {
        ScriptCtx {
            command: command.into(),
            ..Default::default()
        }
    }

    fn plugin(body: &str) -> String {
        format!(
            "export default {{ apiVersion: 1, name: 'Test', commands: [\
             {{ id: 'go', label: 'Go', run(ctx) {{ {body} }} }}] }};"
        )
    }

    fn run_body(grid: &mut VoxelGrid, body: &str) -> Result<Vec<Edit>, ScriptError> {
        run("test.js", &plugin(body), grid, &ctx("go"))
    }

    #[test]
    fn load_reads_the_manifest() {
        let src = "export default { apiVersion: 1, name: 'Trees', commands: [\
                   { id: 'oak', label: 'Oak', run() {} },\
                   { id: 'pine', label: 'Pine', run() {} }] };";
        let m = load("trees.js", src, &Limits::default()).unwrap();
        assert_eq!(m.name, "Trees");
        let ids: Vec<_> = m.commands.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["oak", "pine"]);
        assert_eq!(m.commands[1].label, "Pine");
    }

    #[test]
    fn generator_stages_edits_in_order() {
        let mut grid = VoxelGrid::default();
        let edits = run_body(
            &mut grid,
            "for (let x = 0; x < 3; x++) ctx.grid.set(x, 0, 0, ctx.color);",
        )
        .unwrap();
        let positions: Vec<_> = edits.iter().map(|e| e.pos).collect();
        assert_eq!(
            positions,
            [IVec3::ZERO, IVec3::new(1, 0, 0), IVec3::new(2, 0, 0)]
        );
        assert!(edits.iter().all(|e| e.color == Some([255; 4])));
        assert_eq!(grid.count(), 0, "run never writes the grid");
    }

    const TREE: &str = include_str!("../../docs/src/plugins/examples/tree.js");
    const GRADIENT: &str = include_str!("../../docs/src/plugins/examples/gradient.js");

    #[test]
    fn example_tree_plants_an_oak() {
        let mut grid = VoxelGrid::default();
        let edits = run("tree.js", TREE, &mut grid, &ctx("oak")).unwrap();
        apply(&edits, &mut History::default(), &mut grid);
        let trunk = Some([96, 62, 40, 255]);
        let leaves = Some([124, 190, 60, 255]);
        for y in 0..6 {
            assert_eq!(grid.get(IVec3::new(0, y, 0)), trunk, "y = {y}");
        }
        assert_eq!(grid.get(IVec3::new(0, 7, 0)), leaves);
        assert_eq!(grid.get(IVec3::new(3, 7, 0)), leaves);
        assert_eq!(grid.get(IVec3::new(3, 7, 3)), None);
        let (min, max) = grid.bounding_box().unwrap();
        assert_eq!((min, max), (IVec3::new(-3, 0, -3), IVec3::new(3, 10, 3)));
    }

    #[test]
    fn example_tree_stands_under_the_selection() {
        let mut grid = VoxelGrid::default();
        let cx = ScriptCtx {
            selection: Some(Selection {
                min: IVec3::new(10, 2, 20),
                max: IVec3::new(14, 5, 24),
                mask: None,
            }),
            ..ctx("oak")
        };
        let edits = run("tree.js", TREE, &mut grid, &cx).unwrap();
        apply(&edits, &mut History::default(), &mut grid);
        assert_eq!(grid.get(IVec3::new(12, 2, 22)), Some([96, 62, 40, 255]));
        assert_eq!(grid.bounding_box().unwrap().0, IVec3::new(9, 2, 19));
    }

    fn column(height: i32) -> VoxelGrid {
        let mut grid = VoxelGrid::default();
        for y in 0..height {
            grid.set(IVec3::new(0, y, 0), Some([9, 9, 9, 200]));
        }
        grid
    }

    #[test]
    fn example_gradient_covers_the_whole_model() {
        let mut grid = column(5);
        let cx = ScriptCtx {
            color: [200, 100, 40, 255],
            ..ctx("vertical")
        };
        let edits = run("gradient.js", GRADIENT, &mut grid, &cx).unwrap();
        apply(&edits, &mut History::default(), &mut grid);
        assert_eq!(grid.get(IVec3::ZERO), Some([0, 0, 0, 200]));
        assert_eq!(grid.get(IVec3::new(0, 2, 0)), Some([100, 50, 20, 200]));
        assert_eq!(grid.get(IVec3::new(0, 4, 0)), Some([200, 100, 40, 200]));
        assert_eq!(grid.count(), 5, "empty cells stay empty");
    }

    #[test]
    fn example_gradient_keeps_to_the_selection() {
        let mut grid = column(5);
        let cx = ScriptCtx {
            color: [200, 100, 40, 255],
            selection: Some(Selection {
                min: IVec3::new(-1, 3, -1),
                max: IVec3::new(1, 4, 1),
                mask: None,
            }),
            ..ctx("vertical")
        };
        let edits = run("gradient.js", GRADIENT, &mut grid, &cx).unwrap();
        apply(&edits, &mut History::default(), &mut grid);
        assert_eq!(grid.get(IVec3::new(0, 2, 0)), Some([9, 9, 9, 200]));
        assert_eq!(grid.get(IVec3::new(0, 3, 0)), Some([0, 0, 0, 200]));
        assert_eq!(grid.get(IVec3::new(0, 4, 0)), Some([200, 100, 40, 200]));
        assert_eq!(grid.count(), 5);
    }

    #[test]
    fn apply_writes_one_undo_step() {
        let mut grid = VoxelGrid::default();
        grid.set(IVec3::new(5, 0, 0), Some(RED));
        let mut history = History::default();
        let edits = run_body(
            &mut grid,
            "ctx.grid.set(0, 0, 0, ctx.color); ctx.grid.set(1, 0, 0, ctx.color);\
             ctx.grid.clear(5, 0, 0);",
        )
        .unwrap();
        apply(&edits, &mut history, &mut grid);
        assert_eq!(grid.count(), 2);
        assert_eq!(history.undo.len(), 1);
        history.undo(&mut grid);
        assert_eq!(grid.count(), 1);
        assert_eq!(grid.get(IVec3::new(5, 0, 0)), Some(RED));

        apply(&[], &mut history, &mut grid);
        assert!(history.undo.is_empty(), "an empty run adds no undo step");
    }

    #[test]
    fn recolor_reads_the_grid_and_returns_it_intact() {
        let mut grid = VoxelGrid::default();
        grid.set(IVec3::new(1, 2, 3), Some(RED));
        grid.set(IVec3::new(4, 0, 0), Some(RED));
        let edits = run_body(
            &mut grid,
            "ctx.grid.forEach((x, y, z, [r, g, b]) => ctx.grid.set(x, y, z, [g, r, b]));",
        )
        .unwrap();
        assert_eq!(edits.len(), 2);
        assert!(edits.iter().all(|e| e.color == Some([0, 255, 0, 255])));
        assert_eq!(grid.get(IVec3::new(1, 2, 3)), Some(RED));
        assert_eq!(grid.count(), 2);
    }

    #[test]
    fn reads_see_the_grid_from_before_the_run() {
        let mut grid = VoxelGrid::default();
        let src = plugin(
            "ctx.grid.set(0, 0, 0, [1, 2, 3]);\
             if (ctx.grid.get(0, 0, 0) !== null) throw new Error('saw own write');",
        );
        assert!(run("t.js", &src, &mut grid, &ctx("go")).is_ok());
    }

    #[test]
    fn get_bounds_and_ctx_values() {
        let mut grid = VoxelGrid::default();
        grid.set(IVec3::new(-2, 1, 0), Some(RED));
        grid.set(IVec3::new(3, 4, 5), Some([1, 2, 3, 4]));
        let cx = ScriptCtx {
            command: "go".into(),
            color: [9, 8, 7, 255],
            palette: vec![RED],
            params: vec![
                ("size".into(), Param::Number(4.0)),
                ("hollow".into(), Param::Bool(true)),
                ("kind".into(), Param::Text("oak".into())),
                ("leaf".into(), Param::Color([0, 128, 0, 255])),
            ],
            ..Default::default()
        };
        let src = "export default { apiVersion: 1, name: 'T', commands: [{ id: 'go', label: 'Go',\
             params: [\
               { id: 'size', label: 'Size', type: 'int' },\
               { id: 'hollow', label: 'Hollow', type: 'bool' },\
               { id: 'kind', label: 'Kind', type: 'choice', options: ['pine', 'oak'] },\
               { id: 'leaf', label: 'Leaf', type: 'color' }],\
             run(ctx) {\
             const eq = (a, b) => { if (JSON.stringify(a) !== JSON.stringify(b)) \
                 throw new Error(JSON.stringify(a) + ' != ' + JSON.stringify(b)); };\
             eq(ctx.grid.get(3, 4, 5), [1, 2, 3, 4]);\
             eq(ctx.grid.get(0, 0, 0), null);\
             eq(ctx.grid.bounds(), { min: [-2, 1, 0], max: [3, 4, 5] });\
             eq(ctx.color, [9, 8, 7, 255]);\
             eq(ctx.palette, [[255, 0, 0, 255]]);\
             eq(ctx.params, { size: 4, hollow: true, kind: 'oak', leaf: [0, 128, 0, 255] });\
             eq(ctx.selection, null); } }] };";
        run("t.js", src, &mut grid, &cx).unwrap();
    }

    const PARAMS: &str = "export default { apiVersion: 1, name: 'T', commands: [{ id: 'go', label: 'Go',\
        params: [\
          { id: 'height', label: 'Height', type: 'int', min: 2, max: 20, default: 6 },\
          { id: 'scale', label: 'Scale', type: 'number', min: 0.5 },\
          { id: 'hollow', label: 'Hollow', type: 'bool', default: true },\
          { id: 'leaf', label: 'Leaves', type: 'color', default: [10, 200, 30] },\
          { id: 'kind', label: 'Kind', type: 'choice', options: ['oak', 'pine'], default: 'pine' }],\
        run(ctx) { ctx.grid.set(0, 0, 0, [1, 1, 1]);\
          if (JSON.stringify(ctx.params) !== JSON.stringify(globalThis.want))\
            throw new Error(JSON.stringify(ctx.params)); } }] };";

    #[test]
    fn load_reads_params() {
        let m = load("p.js", PARAMS, &Limits::default()).unwrap();
        let p = &m.commands[0].params;
        let ids: Vec<_> = p.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["height", "scale", "hollow", "leaf", "kind"]);
        assert_eq!(p[0].label, "Height");
        assert_eq!(
            p[0].kind,
            ParamKind::Int {
                default: 6,
                min: Some(2),
                max: Some(20)
            }
        );
        assert_eq!(
            p[1].kind,
            ParamKind::Number {
                default: 0.5,
                min: Some(0.5),
                max: None
            }
        );
        assert_eq!(p[2].kind, ParamKind::Bool { default: true });
        assert_eq!(
            p[3].kind,
            ParamKind::Color {
                default: [10, 200, 30, 255]
            }
        );
        assert_eq!(
            p[4].kind,
            ParamKind::Choice {
                options: vec!["oak".into(), "pine".into()],
                default: 1
            }
        );
        let m = load("t.js", &plugin(""), &Limits::default()).unwrap();
        assert!(m.commands[0].params.is_empty());
    }

    fn run_params(given: Vec<(String, Param)>, want: &str) -> Result<Vec<Edit>, ScriptError> {
        let src = format!("globalThis.want = {want};\n{PARAMS}");
        let cx = ScriptCtx {
            params: given,
            ..ctx("go")
        };
        run("p.js", &src, &mut VoxelGrid::default(), &cx)
    }

    #[test]
    fn a_run_without_values_sees_the_defaults() {
        run_params(
            Vec::new(),
            "{ height: 6, scale: 0.5, hollow: true, leaf: [10, 200, 30, 255], kind: 'pine' }",
        )
        .unwrap();
    }

    #[test]
    fn given_values_are_checked_against_the_declarations() {
        let given = vec![
            ("height".into(), Param::Number(99.4)),
            ("scale".into(), Param::Number(2.25)),
            ("hollow".into(), Param::Text("no".into())),
            ("kind".into(), Param::Text("oak".into())),
            ("extra".into(), Param::Bool(true)),
        ];
        run_params(
            given,
            "{ height: 20, scale: 2.25, hollow: true, leaf: [10, 200, 30, 255], kind: 'oak' }",
        )
        .unwrap();
    }

    #[test]
    fn malformed_params_are_load_errors() {
        let cmd = |params: &str| {
            format!(
                "export default {{ apiVersion: 1, name: 'T', commands: [\
                 {{ id: 'go', label: 'Go', params: {params}, run() {{}} }}] }};"
            )
        };
        let cases = [
            ("{}", "commands[0].params must be an array"),
            ("[1]", "commands[0].params[0] must be an object"),
            ("[{ label: 'A', type: 'bool' }]", "params[0].id must be"),
            ("[{ id: 'a', type: 'bool' }]", "params[0].label must be"),
            ("[{ id: 'a', label: 'A' }]", "params[0].type must be"),
            ("[{ id: 'a', label: 'A', type: 'text' }]", "not \"text\""),
            (
                "[{ id: 'a', label: 'A', type: 'bool' }, { id: 'a', label: 'B', type: 'bool' }]",
                "two params with id \"a\"",
            ),
            (
                "[{ id: 'a', label: 'A', type: 'number', min: '1' }]",
                "min must be a number",
            ),
            (
                "[{ id: 'a', label: 'A', type: 'int', max: 1.5 }]",
                "max must be an integer",
            ),
            (
                "[{ id: 'a', label: 'A', type: 'int', min: 3, max: 1 }]",
                "min is out of range",
            ),
            (
                "[{ id: 'a', label: 'A', type: 'number', max: 1, default: 2 }]",
                "default is out of range",
            ),
            (
                "[{ id: 'a', label: 'A', type: 'bool', default: 1 }]",
                "true or false",
            ),
            (
                "[{ id: 'a', label: 'A', type: 'color', default: '#fff' }]",
                "default must be [r, g, b]",
            ),
            (
                "[{ id: 'a', label: 'A', type: 'choice' }]",
                "options must be an array",
            ),
            (
                "[{ id: 'a', label: 'A', type: 'choice', options: [] }]",
                "must not be empty",
            ),
            (
                "[{ id: 'a', label: 'A', type: 'choice', options: ['x', 'x'] }]",
                "\"x\" twice",
            ),
            (
                "[{ id: 'a', label: 'A', type: 'choice', options: ['x'], default: 'y' }]",
                "default must be one of its options",
            ),
        ];
        for (params, want) in cases {
            let err = load("m.js", &cmd(params), &Limits::default()).unwrap_err();
            assert!(
                matches!(&err, ScriptError::Load(m) if m.contains(want)),
                "{params}: {err}"
            );
        }
    }

    #[test]
    fn empty_grid_has_no_bounds() {
        let mut grid = VoxelGrid::default();
        run_body(
            &mut grid,
            "if (ctx.grid.bounds() !== null) throw new Error('bounds');",
        )
        .unwrap();
    }

    #[test]
    fn selection_mask_narrows_the_box() {
        let mut grid = VoxelGrid::default();
        let mask: HashSet<_> = [IVec3::new(0, 0, 0), IVec3::new(2, 0, 0)].into();
        let cx = ScriptCtx {
            command: "go".into(),
            selection: Some(Selection {
                min: IVec3::ZERO,
                max: IVec3::new(2, 0, 0),
                mask: Some(mask),
            }),
            ..Default::default()
        };
        let src = plugin(
            "if (ctx.selection.has(1, 0, 0)) throw new Error('has');\
             ctx.selection.forEach((x, y, z) => ctx.grid.set(x, y, z, [1, 1, 1]));",
        );
        let mut edits = run("t.js", &src, &mut grid, &cx).unwrap();
        edits.sort_by_key(|e| e.pos.x);
        let xs: Vec<_> = edits.iter().map(|e| e.pos.x).collect();
        assert_eq!(xs, [0, 2]);
    }

    #[test]
    fn selection_without_mask_is_the_whole_box() {
        let mut grid = VoxelGrid::default();
        let cx = ScriptCtx {
            command: "go".into(),
            selection: Some(Selection {
                min: IVec3::ZERO,
                max: IVec3::new(1, 1, 1),
                mask: None,
            }),
            ..Default::default()
        };
        let src = plugin("ctx.selection.forEach((x, y, z) => ctx.grid.set(x, y, z, [1, 1, 1]));");
        assert_eq!(run("t.js", &src, &mut grid, &cx).unwrap().len(), 8);
    }

    #[test]
    fn below_floor_writes_are_dropped() {
        let mut grid = VoxelGrid::default();
        let edits = run_body(
            &mut grid,
            "ctx.grid.set(0, -1, 0, [1, 2, 3]); ctx.grid.set(0, 0, 0, [1, 2, 3]);",
        )
        .unwrap();
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].pos, IVec3::ZERO);
    }

    #[test]
    fn last_write_wins_and_no_ops_are_dropped() {
        let mut grid = VoxelGrid::default();
        grid.set(IVec3::ZERO, Some(RED));
        let edits = run_body(
            &mut grid,
            "ctx.grid.set(1, 0, 0, [1, 2, 3]); ctx.grid.clear(1, 0, 0);\
             ctx.grid.set(0, 0, 0, [255, 0, 0]);\
             ctx.grid.clear(5, 0, 0);\
             ctx.grid.clear(0, 0, 0); ctx.grid.set(0, 0, 0, [0, 0, 255]);",
        )
        .unwrap();
        assert_eq!(
            edits,
            [Edit {
                pos: IVec3::ZERO,
                color: Some([0, 0, 255, 255])
            }]
        );
    }

    #[test]
    fn bad_arguments_throw_type_errors() {
        let mut grid = VoxelGrid::default();
        let err = run_body(&mut grid, "ctx.grid.set(0.5, 0, 0, [1, 2, 3]);").unwrap_err();
        assert!(
            matches!(&err, ScriptError::Run(m) if m.contains("TypeError: grid.set: x must be an integer")),
            "{err}"
        );
        let err = run_body(&mut grid, "ctx.grid.set(0, 0, 0, [1, 2, 300]);").unwrap_err();
        assert!(err.to_string().contains("color must be"), "{err}");
        let err = run_body(&mut grid, "ctx.grid.set(0, 0, 0, '#ff0000');").unwrap_err();
        assert!(err.to_string().contains("color must be"), "{err}");
    }

    #[test]
    fn runtime_errors_carry_the_line() {
        let mut grid = VoxelGrid::default();
        let src = "export default { apiVersion: 1, name: 'T', commands: [{\n\
                   id: 'go', label: 'Go',\n\
                   run(ctx) {\n\
                   undefinedThing.call();\n\
                   } }] };";
        let err = run("bad.js", src, &mut grid, &ctx("go")).unwrap_err();
        assert!(
            matches!(&err, ScriptError::Run(m) if m.starts_with("bad.js:4: ReferenceError")),
            "{err}"
        );
    }

    #[test]
    fn line_in_matches_the_whole_file_name() {
        let stack = "    at f (data.js:12:3)\n    at run (a.js:7:9)\n";
        assert_eq!(line_in(stack, "a.js"), Some(7));
        assert_eq!(line_in(stack, "b.js"), None);
    }

    #[test]
    fn syntax_error_is_a_load_error_with_the_line() {
        let src = "export default {\n  apiVersion: 1,\n  name: 'T' 'oops',\n};";
        let err = load("syntax.js", src, &Limits::default()).unwrap_err();
        assert!(
            matches!(&err, ScriptError::Load(m) if m.starts_with("syntax.js:3: SyntaxError")),
            "{err}"
        );
    }

    #[test]
    fn unknown_api_version_is_refused() {
        let src = "export default { apiVersion: 2, name: 'T', commands: [] };";
        let err = load("v2.js", src, &Limits::default()).unwrap_err();
        assert_eq!(err, ScriptError::UnsupportedApiVersion("2".into()));
        let src = "export default { name: 'T', commands: [] };";
        let err = load("v.js", src, &Limits::default()).unwrap_err();
        assert!(err.to_string().contains("missing apiVersion"), "{err}");
    }

    #[test]
    fn malformed_manifests_are_load_errors() {
        let cases = [
            ("export const x = 1;", "no default export"),
            (
                "export default { apiVersion: 1, commands: [] };",
                "name must be",
            ),
            (
                "export default { apiVersion: 1, name: 'T' };",
                "commands must be an array",
            ),
            (
                "export default { apiVersion: 1, name: 'T', commands: [{ id: 'a', label: 'A' }] };",
                "commands[0].run must be a function",
            ),
            (
                "export default { apiVersion: 1, name: 'T', commands: [\
                 { id: 'a', label: 'A', run() {} }, { id: 'a', label: 'B', run() {} }] };",
                "two commands with id \"a\"",
            ),
        ];
        for (src, want) in cases {
            let err = load("m.js", src, &Limits::default()).unwrap_err();
            assert!(
                matches!(&err, ScriptError::Load(m) if m.contains(want)),
                "{src}: {err}"
            );
        }
    }

    #[test]
    fn unknown_command_is_refused() {
        let mut grid = VoxelGrid::default();
        let err = run("t.js", &plugin(""), &mut grid, &ctx("nope")).unwrap_err();
        assert_eq!(err, ScriptError::UnknownCommand("nope".into()));
    }

    #[test]
    fn plugins_cannot_import() {
        let src = "import fs from 'fs'; export default { apiVersion: 1, name: 'T', commands: [] };";
        assert!(matches!(
            load("i.js", src, &Limits::default()),
            Err(ScriptError::Load(_))
        ));
    }

    #[test]
    fn async_run_is_refused() {
        let src = "export default { apiVersion: 1, name: 'T', commands: [\
                   { id: 'go', label: 'Go', async run(ctx) {} }] };";
        let err = run("a.js", src, &mut VoxelGrid::default(), &ctx("go")).unwrap_err();
        assert!(err.to_string().contains("must be synchronous"), "{err}");
    }

    #[test]
    fn time_limit_stops_a_runaway_script() {
        let mut grid = VoxelGrid::default();
        grid.set(IVec3::ZERO, Some(RED));
        let time = Duration::from_millis(50);
        let cx = ScriptCtx {
            command: "go".into(),
            limits: Limits {
                time,
                ..Limits::default()
            },
            ..Default::default()
        };
        let src = plugin("try { for (;;) {} } catch (e) {}");
        let started = Instant::now();
        let err = run("loop.js", &src, &mut grid, &cx).unwrap_err();
        assert_eq!(err, ScriptError::TimeLimit(time));
        assert!(started.elapsed() < Duration::from_secs(2));
        assert_eq!(
            grid.get(IVec3::ZERO),
            Some(RED),
            "grid comes back after a failed run"
        );
    }

    #[test]
    fn time_limit_covers_the_module_top_level() {
        let limits = Limits {
            time: Duration::from_millis(50),
            ..Limits::default()
        };
        let src = "for (;;) {} export default {};";
        assert!(matches!(
            load("top.js", src, &limits),
            Err(ScriptError::TimeLimit(_))
        ));
    }

    #[test]
    fn memory_limit_stops_a_greedy_script() {
        let limits = Limits {
            memory: 8 << 20,
            ..Limits::default()
        };
        let cx = ScriptCtx {
            command: "go".into(),
            limits,
            ..Default::default()
        };
        let src = plugin("const a = []; for (;;) a.push(new Array(1000).fill(1));");
        let err = run("mem.js", &src, &mut VoxelGrid::default(), &cx).unwrap_err();
        assert_eq!(err, ScriptError::MemoryLimit(8 << 20));
    }

    #[test]
    fn thrown_non_errors_are_run_errors() {
        let err = run_body(&mut VoxelGrid::default(), "throw null;").unwrap_err();
        assert_eq!(err, ScriptError::Run("test.js: uncaught null".into()));
        let err = run_body(&mut VoxelGrid::default(), "throw 'nope';").unwrap_err();
        assert_eq!(err, ScriptError::Run("test.js: uncaught \"nope\"".into()));
    }

    #[test]
    fn memory_limit_holds_even_if_caught() {
        let cx = ScriptCtx {
            command: "go".into(),
            limits: Limits {
                memory: 8 << 20,
                ..Limits::default()
            },
            ..Default::default()
        };
        let src = plugin("try { new Array(1 << 24).fill(1); } catch (e) {}");
        let err = run("mem.js", &src, &mut VoxelGrid::default(), &cx).unwrap_err();
        assert_eq!(err, ScriptError::MemoryLimit(8 << 20));
    }

    /// Every full plugin in the API docs must load, and every command run.
    #[test]
    fn plugin_api_doc_examples_run() {
        let doc = include_str!("../../docs/src/plugin-api.md");
        let mut grid = VoxelGrid::default();
        for p in [IVec3::ZERO, IVec3::new(1, 2, 0), IVec3::new(-3, 0, 4)] {
            grid.set(p, Some(RED));
        }
        let cx = ScriptCtx {
            selection: Some(Selection {
                min: IVec3::ZERO,
                max: IVec3::new(1, 2, 0),
                mask: None,
            }),
            palette: vec![RED, [0, 0, 255, 255]],
            ..Default::default()
        };
        let blocks: Vec<&str> = doc
            .split("```js\n")
            .skip(1)
            .filter_map(|b| b.split("```").next())
            .filter(|b| b.contains("export default"))
            .collect();
        assert!(blocks.len() >= 4, "found {} plugins", blocks.len());
        for src in blocks {
            let manifest =
                load("doc.js", src, &Limits::default()).unwrap_or_else(|e| panic!("{e}\n{src}"));
            for cmd in manifest.commands {
                let cx = ScriptCtx {
                    command: cmd.id,
                    ..cx.clone()
                };
                let edits =
                    run("doc.js", src, &mut grid, &cx).unwrap_or_else(|e| panic!("{e}\n{src}"));
                assert!(!edits.is_empty(), "{src}");
            }
        }
    }

    #[test]
    fn edit_limit_stops_the_run_even_if_caught() {
        let cx = ScriptCtx {
            command: "go".into(),
            limits: Limits {
                edits: 10,
                ..Limits::default()
            },
            ..Default::default()
        };
        let src = plugin(
            "try { for (let x = 0; x < 11; x++) ctx.grid.set(x, 0, 0, [1, 1, 1]); } catch (e) {}",
        );
        let err = run("many.js", &src, &mut VoxelGrid::default(), &cx).unwrap_err();
        assert_eq!(err, ScriptError::EditLimit(10));
    }
}
