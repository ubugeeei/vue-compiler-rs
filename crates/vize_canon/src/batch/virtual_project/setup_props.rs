use std::path::Path;

use vize_atelier_sfc::{SfcDescriptor, croquis::merge_resolved_props_into_croquis};
use vize_carton::FxHashSet;
use vize_croquis::{BindingType, Croquis};

mod cache;
mod default_props;
mod imports;
mod runtime_props;
mod syntax;

pub(super) use cache::RuntimePropResolveCache;

use default_props::merge_type_based_with_defaults_into_croquis;
use imports::find_imported_runtime_binding;
use runtime_props::resolve_imported_runtime_props;
use syntax::runtime_arg_identifier;

pub(super) fn augment_type_based_props_from_script_context(
    croquis: &mut Croquis,
    descriptor: &SfcDescriptor<'_>,
    path: &Path,
    cache: &RuntimePropResolveCache,
) {
    let path_string = path.to_string_lossy();
    merge_resolved_props_into_croquis(croquis, descriptor, path_string.as_ref());
    merge_imported_runtime_props_into_croquis(croquis, descriptor, path, cache);

    let script_setup_source = descriptor
        .script_setup
        .as_ref()
        .map(|script| script.content.as_ref());
    if let Some(script_setup_source) = script_setup_source {
        merge_type_based_with_defaults_into_croquis(
            croquis,
            script_setup_source,
            descriptor
                .script
                .as_ref()
                .map(|script| script.content.as_ref()),
            path,
            cache,
        );
    }
}

fn merge_imported_runtime_props_into_croquis(
    croquis: &mut Croquis,
    descriptor: &SfcDescriptor<'_>,
    path: &Path,
    cache: &RuntimePropResolveCache,
) {
    let Some(script_setup) = descriptor.script_setup.as_ref() else {
        return;
    };
    let Some(runtime_arg) = croquis
        .macros
        .define_props()
        .filter(|call| call.type_args.is_none())
        .and_then(|call| call.runtime_args.as_deref())
        .and_then(runtime_arg_identifier)
    else {
        return;
    };

    let import = find_imported_runtime_binding(&script_setup.content, runtime_arg).or_else(|| {
        descriptor
            .script
            .as_ref()
            .and_then(|script| find_imported_runtime_binding(&script.content, runtime_arg))
    });
    let Some(import) = import else {
        return;
    };

    let mut visited = FxHashSet::default();
    let props = resolve_imported_runtime_props(
        path,
        import.source.as_str(),
        import.imported.as_str(),
        runtime_arg,
        &mut visited,
        cache,
    );
    if props.is_empty() {
        return;
    }

    let mut known: FxHashSet<_> = croquis
        .macros
        .props()
        .iter()
        .map(|prop| prop.name.clone())
        .collect();
    for prop in props {
        if !known.insert(prop.name.clone()) {
            continue;
        }
        if !croquis.bindings.contains(prop.name.as_str()) {
            croquis.bindings.add(prop.name.as_str(), BindingType::Props);
        }
        croquis.macros.add_prop(prop);
    }
}
