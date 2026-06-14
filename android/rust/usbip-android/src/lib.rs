// Minimal JNI bridge for AnyPlug Android native library.
// This crate exposes USB/IP protocol functions to the Android Java/Kotlin layer.
// Scaffold only — real implementation to be filled in.
use jni::JNIEnv;
use jni::objects::JClass;
use jni::sys::jstring;

/// Placeholder: returns a version string.
#[no_mangle]
pub extern "system" fn Java_com_anyplug_NativeBridge_getVersion<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> jstring {
    let version = env!("CARGO_PKG_VERSION");
    env.new_string(version)
        .expect("Failed to create Java string")
        .into_raw()
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_works() {
        assert_eq!(env!("CARGO_PKG_VERSION"), "0.1.0");
    }
}
