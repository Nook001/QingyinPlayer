use cstr::cstr;
use qmetaobject::prelude::*;

#[allow(missing_debug_implementations)]
#[derive(QObject, Default)]
pub struct AppBridge {
    base: qt_base_class!(trait QObject),
    application_name: qt_method!(
        fn application_name(&self) -> QString {
            let _ = self;
            "清音".into()
        }
    ),
    version: qt_method!(
        fn version(&self) -> QString {
            let _ = self;
            env!("CARGO_PKG_VERSION").into()
        }
    ),
}

pub fn register_qml_types() {
    qml_register_type::<AppBridge>(cstr!("Qingyin"), 1, 0, cstr!("AppBridge"));
}
