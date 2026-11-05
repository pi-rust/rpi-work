//! Android-only remembered-connection transport. Kotlin owns Keystore and disk I/O;
//! the caller owns SessionEnvelope/RememberedSession validation and auth policy.

use jni::{
    JavaVM,
    objects::{JObject, JString, JValue},
};
use serde_json::Value;

enum Argument<'a> {
    None,
    Payload(&'a str),
    KeepAddress(bool),
}

#[allow(unsafe_code)] // Only borrows the VM/Activity owned by ndk-context.
fn activity_call(method: &str, argument: Argument<'_>) -> Result<String, ()> {
    let context = ndk_context::android_context();
    if context.vm().is_null() || context.context().is_null() {
        return Err(());
    }
    // SAFETY: ndk-context retains this non-null VM pointer throughout the native
    // Activity lifetime; JavaVM does not destroy the borrowed VM on drop.
    let vm = unsafe { JavaVM::from_raw(context.vm().cast()) }.map_err(|_| ())?;
    let mut env = vm.attach_current_thread().map_err(|_| ())?;
    // SAFETY: this non-null Activity is ndk-context's live global JNI reference.
    // JObject does not delete it; our local frame releases only our own strings.
    let activity = unsafe { JObject::from_raw(context.context().cast()) };
    let result = env.with_local_frame(8, |env| -> jni::errors::Result<String> {
        let result = match argument {
            Argument::None => env.call_method(&activity, method, "()Ljava/lang/String;", &[]),
            Argument::Payload(payload) => {
                let payload = env.new_string(payload)?;
                env.call_method(
                    &activity,
                    method,
                    "(Ljava/lang/String;)Ljava/lang/String;",
                    &[JValue::Object(&payload)],
                )
            }
            Argument::KeepAddress(keep_address) => env.call_method(
                &activity,
                method,
                "(Z)Ljava/lang/String;",
                &[JValue::Bool(u8::from(keep_address))],
            ),
        }?
        .l()?;
        let result = JString::from(result);
        let text = env.get_string(&result)?;
        Ok(String::from(text))
    });
    if result.is_err() || env.exception_check().unwrap_or(true) {
        // Never describe/log Java exceptions: they may contain credential data.
        let _ = env.exception_clear();
        return Err(());
    }
    let text = result.map_err(|_| ())?;
    if text.len() > 131_072 {
        return Err(());
    }
    Ok(text)
}

pub(super) fn load() -> Result<Value, ()> {
    let text = activity_call("loadRememberedConnection", Argument::None)?;
    serde_json::from_str(&text).map_err(|_| ())
}

pub(super) fn save(payload: &str) -> bool {
    if payload.len() > 32768 {
        return false;
    }
    activity_call("saveRememberedConnection", Argument::Payload(payload))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.get("saved").and_then(Value::as_bool))
        == Some(true)
}

pub(super) fn clear(keep_address: bool) -> bool {
    activity_call(
        "clearRememberedConnection",
        Argument::KeepAddress(keep_address),
    )
    .ok()
    .and_then(|text| serde_json::from_str::<Value>(&text).ok())
    .and_then(|value| value.get("cleared").and_then(Value::as_bool))
        == Some(true)
}
