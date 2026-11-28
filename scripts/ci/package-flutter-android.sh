#!/usr/bin/env bash
set -euo pipefail

build_tools="${ANDROID_HOME:?}/build-tools/36.0.0"
unsigned=apps/work-mobile/build/app/outputs/flutter-apk/app-arm64-v8a-release.apk
dist=target/ci-dist
scratch=$(mktemp -d "${RUNNER_TEMP:?}/flutter-android.XXXXXX")
trap 'rm -rf -- "$scratch"' EXIT
keystore="$scratch/signing.p12"

if [[ "$GITHUB_REF_TYPE" == tag ]]; then
  : "${ANDROID_KEYSTORE_BASE64:?Android release signing keystore is required}"
  : "${ANDROID_KEYSTORE_PASSWORD:?ANDROID_KEYSTORE_PASSWORD is required for a tagged release}"
  printf '%s' "$ANDROID_KEYSTORE_BASE64" | base64 --decode > "$keystore"
  unset ANDROID_KEYSTORE_BASE64
  alias=rpi-work
  apk=rpi-work-android-arm64.apk
else
  export ANDROID_KEYSTORE_PASSWORD=android
  alias=androiddebugkey
  apk=rpi-work-android-arm64-dev.apk
  keytool -genkeypair -keystore "$keystore" -storetype PKCS12 \
    -storepass:env ANDROID_KEYSTORE_PASSWORD -alias "$alias" \
    -keyalg RSA -keysize 2048 -validity 10000 -dname 'CN=Android Debug'
fi

"$build_tools/zipalign" -P 16 -f 4 "$unsigned" "$scratch/aligned.apk"
"$build_tools/apksigner" sign --ks "$keystore" --ks-type PKCS12 --ks-key-alias "$alias" \
  --ks-pass env:ANDROID_KEYSTORE_PASSWORD --key-pass env:ANDROID_KEYSTORE_PASSWORD \
  --out "$scratch/$apk" "$scratch/aligned.apk"
"$build_tools/apksigner" verify --verbose --print-certs "$scratch/$apk"

badging=$("$build_tools/aapt" dump badging "$scratch/$apk")
if ! grep -q "^package: name='com\.pirust\.rpiwork\.mobile' " <<< "$badging" ||
   ! grep -qx "native-code: 'arm64-v8a'" <<< "$badging"; then
  echo 'Expected the RPI Work mobile application ID and the ARM64 ABI.' >&2
  exit 1
fi
native_files=$(unzip -Z1 "$scratch/$apk")
for library in libapp.so libflutter.so; do
  grep -Fxq "lib/arm64-v8a/$library" <<< "$native_files"
done
if [[ "$GITHUB_REF_TYPE" == tag ]] &&
   [[ "$badging" != *"versionName='${GITHUB_REF_NAME#v}'"* ]]; then
  echo 'The APK version does not match the release tag.' >&2
  exit 1
fi

mkdir -p "$dist"
cp -- "$scratch/$apk" "$dist/$apk"
cd "$dist"
sha256sum -- "$apk" > "$apk.sha256"
