// Source wiring only, not a substitute for Android Keystore/device execution.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const { test } = require('node:test');
const source = file => fs.readFileSync(file, 'utf8');
const entry = source('apps/work-ui/src/app/mobile_entry.rs');
const kotlin = source('apps/work-ui/android/MainActivity.kt');
const bridge = source('apps/work-ui/src/app/connection_memory_android.rs');
const storage = kotlin.slice(kotlin.indexOf('    // Only the non-sensitive root address'), kotlin.indexOf('    @Suppress("DEPRECATION")'));

test('native credential storage uses authenticated encryption, non-exportable key and no backups', () => {
  assert.match(source('apps/work-ui/android/AndroidManifest.xml'), /android:allowBackup="false"/);
  for (const text of ['AndroidKeyStore', 'AES/GCM/NoPadding', 'setKeySize(256)', 'setRandomizedEncryptionRequired(true)', 'noBackupFilesDir', 'AtomicFile', 'GCMParameterSpec(128', 'check(cipher.iv.size == 12)', 'cipher.updateAAD(rememberedAad())', 'plain?.fill(0)']) assert(storage.includes(text), text);
  assert.doesNotMatch(storage, /SharedPreferences|localStorage|Log\.|println\(|error\.message|exception\.message/);
  assert.match(storage, /fields == setOf\("version", "address", "nodeId", "session"\)/);
  assert.match(storage, /rememberedMaxPayload = 32768/);
  assert.match(storage, /check\(envelope.getString\("address"\) == address\)/);
  assert.match(storage, /rememberedDelete\(rememberedSessionFile\(\)\)/);
});

test('JNI sends credential data directly to kept Kotlin methods, never WebView eval', () => {
  for (const method of ['loadRememberedConnection', 'saveRememberedConnection', 'clearRememberedConnection']) {
    assert(bridge.includes(`"${method}"`));
    assert.match(storage, new RegExp(`@androidx.annotation.Keep\\s+fun ${method}\\(`));
  }
  assert(bridge.includes('(Z)Ljava/lang/String;'));
  assert(bridge.includes('env.with_local_frame'));
  assert(bridge.includes('env.exception_clear()'));
  assert.doesNotMatch(bridge, /document::eval|println!|dbg!|to_string\(\)/);
});

test('entry guards both restoration and successful login persistence by current attempt', () => {
  const restore = entry.slice(entry.indexOf('let result = attempt.resume'), entry.indexOf('let startup_resume'));
  assert(restore.indexOf('if epoch.get() != generation') < restore.indexOf('authorized.set(Some(node))'));
  assert(restore.indexOf('if epoch.get() != generation') < restore.indexOf('secure_store::clear(true)'));
  assert(!restore.includes('remember('), 'restoration must not extend stored TTL');
  const login = entry.slice(entry.indexOf('let result = attempt.login'), entry.indexOf('let mut scan ='));
  assert(login.indexOf('if epoch.get() != generation') < login.indexOf('remember(&node, node_id)'));
  assert(login.includes('connection_memory::identity(&node.address).await'));
  const qr = entry.slice(entry.indexOf('client.redeem_qr_authorization'), entry.indexOf('if let Some(node) = authorized.read()'));
  assert(qr.indexOf('if epoch.get() != attempt') < qr.indexOf('remember(&node, Some(node_id))'));
  assert(entry.includes('if startup_epoch.get() == 0'));
});

test('forget deletes before navigation; cancellation invalidates old requests without clearing stored login', () => {
  const forget = entry.slice(entry.indexOf('let forget ='), entry.indexOf('use_context_provider'));
  assert(forget.indexOf('secure_store::clear(false)') < forget.indexOf('authorized.set(None)'));
  const cancel = entry.slice(entry.indexOf('let mut cancel ='), entry.indexOf('let mut restart'));
  assert(cancel.includes('epoch.set(epoch.get().wrapping_add(1))'));
  assert(cancel.includes('old.cancel()'));
  assert(!cancel.includes('secure_store::clear'));
  assert(source('apps/work-ui/src/features/settings/connection_panel.rs').includes('"忘记已保存的电脑"'));
  assert(source('apps/work-ui/src/features/settings/devices.rs').includes('control.forget'));
});
