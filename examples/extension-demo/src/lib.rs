//! A real rpi cdylib. It only counts in memory; no filesystem, network or model access.
use std::{
    ffi::c_void,
    sync::atomic::{AtomicU32, Ordering},
    time::{Duration, Instant},
};

use rpi_plugin_sdk::{
    StableToolSchema, StbString, StbStringRef, StepHandle, StepResult, ToolPartialCb,
    callbacks::free_string, handles::ToolHandles,
};
use serde_json::{Value, json};

static TOTAL: AtomicU32 = AtomicU32::new(0);
static DRIVES: ToolHandles<Drive> = ToolHandles::new();

struct Drive {
    steps: u32,
    completed: u32,
    next_tick: Instant,
    error: Option<String>,
}

impl Drive {
    fn new(params: &str) -> Self {
        let steps = serde_json::from_str::<Value>(params)
            .ok()
            .and_then(|value| value["steps"].as_u64())
            .filter(|steps| (1..=10).contains(steps))
            .and_then(|steps| u32::try_from(steps).ok());
        Self {
            steps: steps.unwrap_or(0),
            completed: 0,
            next_tick: Instant::now(),
            error: steps
                .is_none()
                .then(|| "steps must be an integer from 1 to 10".into()),
        }
    }
}

rpi_plugin_sdk::execute_handler! {
    fn execute(_tool_call_id, params) {
        DRIVES.insert(Drive::new(&params.to_string_lossy()))
    }
}

extern "C" fn poll(
    handle: StepHandle,
    partial: Option<ToolPartialCb>,
    user: *mut c_void,
) -> StepResult {
    rpi_plugin_sdk::guard(|| {
        let Some(entry) = DRIVES.get(handle) else {
            return StepResult::err(StbString::from_string("invalid handle".into()));
        };
        if entry.cancelled.load(Ordering::Acquire) {
            return StepResult::err(StbString::from_string("counter cancelled".into()));
        }
        let (body, done) = {
            let mut drive = entry.lock();
            if let Some(error) = &drive.error {
                return StepResult::err(StbString::from_string(error.clone()));
            }
            if Instant::now() < drive.next_tick {
                return StepResult::pending(StbString::empty());
            }
            drive.completed += 1;
            drive.next_tick = Instant::now() + Duration::from_millis(180);
            let total = TOTAL.fetch_add(1, Ordering::SeqCst) + 1;
            let done = drive.completed == drive.steps;
            let body = json!({
                "content":[{"type":"text","text":format!("Actual DLL counter: {total} ({}/{})", drive.completed, drive.steps)}],
                "details":{"demo":"rpi-work-counter-v1","count":total,"completed":drive.completed,"steps":drive.steps,"done":done}
            }).to_string();
            (body, done)
        };
        // Never hold the tool state mutex while calling back into the host.
        if done {
            StepResult::done(StbString::from_string(body))
        } else {
            if let Some(partial) = partial {
                partial(StbString::from_string(body), user);
            }
            StepResult::pending(StbString::empty())
        }
    }).unwrap_or_else(|_| StepResult::err(StbString::from_string("counter poll panicked".into())))
}

extern "C" fn cancel(handle: StepHandle) {
    DRIVES.cancel(handle);
}

extern "C" fn destroy(handle: StepHandle) {
    drop(DRIVES.remove(handle));
}

rpi_plugin_sdk::string_handler! {
    fn status(_args, out, _user) {
        i32::from(!out.write(json!({"kind":"message","text":"Real cdylib loaded","count":TOTAL.load(Ordering::SeqCst)}).to_string()))
    }
}

// SDK 0.8.3 generates a checked, safe extern C entry from a raw PluginApi pointer.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
mod abi {
    use super::*;
    rpi_plugin_sdk::export_plugin!(|api| {
        let (Some(register_tool), Some(register_command)) =
            (api.register_tool, api.register_command)
        else {
            return 1;
        };
        let schema = StableToolSchema {
        name: StbString::from_string("work_demo_counter".into()),
        description: StbString::from_string("Count 1–10 ticks in memory with real partial updates. Demo only.".into()),
        parameters: StbString::from_string(r#"{"type":"object","properties":{"steps":{"type":"integer","minimum":1,"maximum":10}},"required":["steps"]}"#.into()),
    };
        let result = register_tool(&schema, execute, poll, cancel, destroy, free_string);
        // The registrar adopts/frees schema strings using the supplied callback.
        if result != 0 {
            return result;
        }
        register_command(
            StbStringRef::from_str("work-demo-status"),
            StbStringRef::from_str("Report the actual loaded DLL counter"),
            status,
        )
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_is_idempotent_and_destroy_retires_the_handle() {
        let handle = DRIVES.insert(Drive::new(r#"{"steps":4}"#));
        cancel(handle);
        cancel(handle);
        assert!(
            DRIVES
                .get(handle)
                .unwrap()
                .cancelled
                .load(Ordering::Acquire)
        );
        destroy(handle);
        destroy(handle);
        assert!(DRIVES.get(handle).is_none());
    }

    #[test]
    fn accepts_host_context_but_bounds_steps() {
        let drive = Drive::new(r#"{"steps":4,"__rpi":{"sessionId":"isolated-demo"}}"#);
        assert_eq!(drive.steps, 4);
        assert!(drive.error.is_none());
        for invalid in [
            "{}",
            "[]",
            "bad json",
            r#"{"steps":0}"#,
            r#"{"steps":11}"#,
            r#"{"steps":1.5}"#,
            r#"{"steps":-1}"#,
        ] {
            assert!(Drive::new(invalid).error.is_some(), "{invalid}");
        }
    }
}
