use rong::*;
use std::cell::Cell;
use std::collections::BTreeSet;

#[cfg(feature = "worker")]
pub use rong_worker as worker;

/// Metadata for a module compiled into `rong_modules`.
#[derive(Clone, Copy, Debug)]
pub struct ModuleDescriptor {
    name: &'static str,
    dependencies: &'static [&'static str],
    init: fn(&JSContext) -> JSResult<()>,
}

impl ModuleDescriptor {
    const fn new(
        name: &'static str,
        dependencies: &'static [&'static str],
        init: fn(&JSContext) -> JSResult<()>,
    ) -> Self {
        Self {
            name,
            dependencies,
            init,
        }
    }

    /// Stable module name, matching its `rong_modules` Cargo feature.
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Direct runtime dependencies installed with this module.
    pub const fn dependencies(&self) -> &'static [&'static str] {
        self.dependencies
    }
}

#[cfg(test)]
#[derive(Clone, Copy)]
struct ModuleSpec {
    name: &'static str,
    dependencies: &'static [&'static str],
}

macro_rules! define_modules {
    ($(($feature:literal, $name:literal, [$($dependency:literal),* $(,)?], $init:path)),* $(,)?) => {
        // This ungated catalog lets tests validate the complete graph even when
        // the current test build enables only a subset of module features.
        #[cfg(test)]
        static MODULE_SPECS: &[ModuleSpec] = &[
            $(ModuleSpec {
                name: $name,
                dependencies: &[$($dependency),*],
            }),*
        ];

        static COMPILED_MODULES: &[ModuleDescriptor] = &[
            $(
                #[cfg(feature = $feature)]
                ModuleDescriptor::new($name, &[$($dependency),*], $init)
            ),*
        ];
    };
}

// Keep this list in dependency-first initialization order. The tests below
// enforce uniqueness, dependency closure, and ordering.
define_modules!(
    ("timer", "timer", [], rong_timer::init),
    ("cron", "cron", [], rong_cron::init),
    ("event", "event", [], rong_event::init),
    ("exception", "exception", [], rong_exception::init),
    ("abort", "abort", [], rong_abort::init),
    ("encoding", "encoding", [], rong_encoding::init),
    ("console", "console", [], rong_console::init),
    ("url", "url", [], rong_url::init),
    ("buffer", "buffer", [], rong_buffer::init),
    ("stream", "stream", ["abort"], rong_stream::init),
    ("compression", "compression", [], rong_compression::init),
    (
        "command",
        "command",
        ["abort", "buffer", "encoding", "stream"],
        rong_command::init
    ),
    (
        "http",
        "http",
        ["abort", "exception", "buffer", "url", "stream"],
        rong_http::init
    ),
    ("assert", "assert", [], rong_assert::init),
    ("fs", "fs", ["abort", "exception", "stream"], rong_fs::init),
    ("storage", "storage", [], rong_storage::init),
    ("redis", "redis", ["abort"], rong_redis::init),
    ("sqlite", "sqlite", [], rong_sqlite::init),
    ("worker", "worker", [], rong_worker::init),
    ("s3", "s3", [], rong_s3::init),
);

const _: () = assert!(COMPILED_MODULES.len() <= u64::BITS as usize);

#[derive(Default)]
struct InitializedModules(Cell<u64>);

fn descriptor_index(name: &str) -> Option<usize> {
    COMPILED_MODULES
        .iter()
        .position(|module| module.name == name)
}

fn resolve_module_mask<I, S>(requested: I) -> JSResult<u64>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut selected = 0_u64;
    let mut missing = BTreeSet::new();

    for requested_name in requested {
        let requested_name = requested_name.as_ref();
        if let Some(index) = descriptor_index(requested_name) {
            selected |= 1 << index;
        } else {
            missing.insert(requested_name.to_owned());
        }
    }

    if !missing.is_empty() {
        let available = compiled_module_names().collect::<Vec<_>>().join(", ");
        return Err(HostError::new(
            rong::error::E_INVALID_ARG,
            format!(
                "Modules are not compiled into this build: {}. Available modules: {available}",
                missing.into_iter().collect::<Vec<_>>().join(", ")
            ),
        )
        .with_name("TypeError")
        .into());
    }

    loop {
        let previous = selected;
        for (index, module) in COMPILED_MODULES.iter().enumerate() {
            if selected & (1 << index) == 0 {
                continue;
            }
            for dependency in module.dependencies {
                let dependency_index = descriptor_index(dependency).ok_or_else(|| {
                    HostError::new(
                        rong::error::E_INTERNAL,
                        format!(
                            "Module registry is invalid: {} depends on uncompiled module {dependency}",
                            module.name
                        ),
                    )
                })?;
                selected |= 1 << dependency_index;
            }
        }
        if selected == previous {
            break;
        }
    }

    Ok(selected)
}

fn initialized_modules(ctx: &JSContext) -> &InitializedModules {
    if ctx.get_state::<InitializedModules>().is_none() {
        ctx.set_state(InitializedModules::default());
    }
    ctx.get_state::<InitializedModules>()
        .expect("module state was inserted above")
}

/// All modules compiled into this build, in dependency-first init order.
pub const fn compiled_modules() -> &'static [ModuleDescriptor] {
    COMPILED_MODULES
}

/// Names of all modules compiled into this build.
pub fn compiled_module_names() -> impl ExactSizeIterator<Item = &'static str> + DoubleEndedIterator
{
    COMPILED_MODULES.iter().map(ModuleDescriptor::name)
}

/// Whether a module is compiled into this build.
pub fn is_compiled(name: &str) -> bool {
    descriptor_index(name).is_some()
}

/// Resolve requested modules to their dependency-closed initialization order.
///
/// Validation is fail-fast: an unknown or uncompiled name returns an error
/// before a JavaScript context is modified.
pub fn resolve_modules<I, S>(requested: I) -> JSResult<Vec<&'static str>>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let selected = resolve_module_mask(requested)?;
    Ok(COMPILED_MODULES
        .iter()
        .enumerate()
        .filter(|(index, _)| selected & (1 << index) != 0)
        .map(|(_, module)| module)
        .map(ModuleDescriptor::name)
        .collect())
}

/// Initialize a dependency-closed module set in a JavaScript context.
///
/// Modules already initialized through this registry in the same context are
/// skipped. Direct calls to individual module crates are outside this registry
/// and therefore cannot be tracked here.
pub fn init<I, S>(ctx: &JSContext, modules: I) -> JSResult<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let selected = resolve_module_mask(modules)?;
    if selected == 0 {
        return Ok(());
    }
    let state = initialized_modules(ctx);
    let mut initialized = state.0.get();

    for (index, module) in COMPILED_MODULES.iter().enumerate() {
        let bit = 1 << index;
        if selected & bit == 0 || initialized & bit != 0 {
            continue;
        }
        (module.init)(ctx)?;
        initialized |= bit;
        state.0.set(initialized);
    }

    Ok(())
}

/// Initialize every module compiled into this build.
pub fn init_all(ctx: &JSContext) -> JSResult<()> {
    init(ctx, compiled_module_names())
}

/// Modules initialized through this registry in `ctx`, in canonical order.
pub fn initialized_module_names(ctx: &JSContext) -> Vec<&'static str> {
    let Some(state) = ctx.get_state::<InitializedModules>() else {
        return Vec::new();
    };
    let initialized = state.0.get();
    COMPILED_MODULES
        .iter()
        .enumerate()
        .filter(|(index, _)| initialized & (1 << index) != 0)
        .map(|(_, module)| module)
        .map(ModuleDescriptor::name)
        .collect()
}

/// Whether this registry initialized `name` in `ctx`.
pub fn is_initialized(ctx: &JSContext, name: &str) -> bool {
    let Some(index) = descriptor_index(name) else {
        return false;
    };
    ctx.get_state::<InitializedModules>()
        .is_some_and(|state| state.0.get() & (1 << index) != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_and_dependencies_are_ordered() {
        assert!(MODULE_SPECS.len() <= u64::BITS as usize);
        let mut names = BTreeSet::new();
        for module in MODULE_SPECS {
            assert!(
                names.insert(module.name),
                "duplicate module: {}",
                module.name
            );
            for dependency in module.dependencies {
                assert!(
                    names.contains(dependency),
                    "{} depends on missing or later module {dependency}",
                    module.name
                );
            }
        }
    }

    #[test]
    fn every_compiled_module_resolves_with_its_dependencies() {
        for (index, module) in COMPILED_MODULES.iter().enumerate() {
            let resolved = resolve_module_mask([module.name]).unwrap();
            assert_ne!(resolved & (1 << index), 0);
            for dependency in module.dependencies {
                let dependency_index = descriptor_index(dependency).unwrap();
                assert_ne!(resolved & (1 << dependency_index), 0);
            }
        }
    }

    #[test]
    fn unknown_modules_fail_resolution() {
        assert!(resolve_module_mask(["definitely-not-a-module"]).is_err());
    }
}
