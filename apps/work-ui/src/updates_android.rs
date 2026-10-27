use jni::{
    JavaVM,
    objects::{JObject, JString, JValue},
};
use std::path::PathBuf;

#[allow(unsafe_code)] // JNI borrows VM/Activity pointers owned by ndk-context.
fn activity_call(method: &str, argument: Option<&str>) -> Result<String, String> {
    let context = ndk_context::android_context();
    // ndk-context owns the VM and Activity for the entire native activity lifetime.
    // JObject is used as a borrowed JNI reference and is not deleted by Rust.
    let vm = unsafe { JavaVM::from_raw(context.vm().cast()) }.map_err(|e| e.to_string())?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let activity = unsafe { JObject::from_raw(context.context().cast()) };
    let result = if let Some(argument) = argument {
        let argument = env.new_string(argument).map_err(|e| e.to_string())?;
        env.call_method(
            &activity,
            method,
            "(Ljava/lang/String;)Ljava/lang/String;",
            &[JValue::Object(&argument)],
        )
    } else {
        env.call_method(&activity, method, "()Ljava/lang/String;", &[])
    }
    .and_then(|value| value.l())
    .map_err(|e| e.to_string())?;
    let result = JString::from(result);
    env.get_string(&result)
        .map(String::from)
        .map_err(|e| e.to_string())
}

pub fn cache_root() -> Result<PathBuf, String> {
    activity_call("updateCacheDirectory", None).map(PathBuf::from)
}

pub fn install(prepared: &work_update::native::Prepared) -> Result<(), String> {
    let message = activity_call(
        "installVerifiedUpdate",
        Some(&prepared.directory.join("update.apk").to_string_lossy()),
    )?;
    if message.is_empty() {
        Ok(())
    } else {
        Err(message)
    }
}
