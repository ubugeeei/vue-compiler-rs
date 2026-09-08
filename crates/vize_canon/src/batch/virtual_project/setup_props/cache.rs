use std::sync::Mutex;

use vize_carton::{CompactString, FxHashMap, FxHashSet};
use vize_croquis::macros::PropDefinition;

use super::syntax::runtime_prop_shape_member_type;

#[derive(Default)]
pub(crate) struct RuntimePropResolveCache {
    props: Mutex<FxHashMap<CompactString, Vec<PropDefinition>>>,
    default_names: Mutex<FxHashMap<CompactString, FxHashSet<CompactString>>>,
}

impl RuntimePropResolveCache {
    pub(super) fn props(
        &self,
        key: &CompactString,
        root_runtime_binding: &str,
    ) -> Option<Vec<PropDefinition>> {
        let props = self.props.lock().ok()?.get(key).cloned()?;
        Some(attach_runtime_prop_types(props, root_runtime_binding))
    }

    pub(super) fn insert_props(&self, key: CompactString, props: Vec<PropDefinition>) {
        if let Ok(mut cache) = self.props.lock() {
            cache.insert(key, detach_runtime_prop_types(props));
        }
    }

    pub(super) fn default_names(&self, key: &CompactString) -> Option<FxHashSet<CompactString>> {
        self.default_names.lock().ok()?.get(key).cloned()
    }

    pub(super) fn insert_default_names(&self, key: CompactString, names: FxHashSet<CompactString>) {
        if let Ok(mut cache) = self.default_names.lock() {
            cache.insert(key, names);
        }
    }
}

fn detach_runtime_prop_types(props: Vec<PropDefinition>) -> Vec<PropDefinition> {
    props
        .into_iter()
        .map(|mut prop| {
            prop.prop_type = None;
            prop
        })
        .collect()
}

fn attach_runtime_prop_types(
    props: Vec<PropDefinition>,
    root_runtime_binding: &str,
) -> Vec<PropDefinition> {
    props
        .into_iter()
        .map(|mut prop| {
            prop.prop_type = Some(runtime_prop_shape_member_type(
                root_runtime_binding,
                prop.name.as_str(),
            ));
            prop
        })
        .collect()
}
