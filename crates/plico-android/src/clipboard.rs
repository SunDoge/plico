use super::AndroidApp;
use jni::{
    JavaVM, jni_sig, jni_str,
    objects::{JObject, JString, JValue},
    refs::Global,
};
use plico_renderer::platform::{EffectErrorCode, EffectResult};

pub(super) fn failure(_: jni::errors::Error) -> EffectResult {
    EffectResult::Error {
        code: EffectErrorCode::PlatformFailure,
        message: "Android clipboard service failed".into(),
    }
}

pub(super) fn text(app: &AndroidApp, write: Option<&str>) -> jni::errors::Result<Option<String>> {
    JavaVM::singleton()?.attach_current_thread(|env| {
        let raw = app.activity_as_ptr().cast();
        // SAFETY: AndroidApp owns the global reference for this entire callback.
        let activity = unsafe { env.as_cast_raw::<Global<JObject>>(&raw)? };
        let name = env.new_string("clipboard")?;
        let manager = env.call_method(activity, jni_str!("getSystemService"), jni_sig!((java.lang.String) -> java.lang.Object), &[JValue::Object(&name)])?.l()?;
        if let Some(value) = write {
            let value = env.new_string(value)?;
            let clip = env.call_static_method(jni_str!("android/content/ClipData"), jni_str!("newPlainText"), jni_sig!((java.lang.CharSequence, java.lang.CharSequence) -> android.content.ClipData), &[JValue::Object(&JObject::null()), JValue::Object(&value)])?.l()?;
            env.call_method(&manager, jni_str!("setPrimaryClip"), jni_sig!((android.content.ClipData) -> void), &[JValue::Object(&clip)])?;
            return Ok(None);
        }
        let clip = env.call_method(&manager, jni_str!("getPrimaryClip"), jni_sig!(() -> android.content.ClipData), &[])?.l()?;
        if clip.is_null() { return Ok(None); }
        let item = env.call_method(&clip, jni_str!("getItemAt"), jni_sig!("(I)Landroid/content/ClipData$Item;"), &[JValue::Int(0)])?.l()?;
        let chars = env.call_method(&item, jni_str!("getText"), jni_sig!(() -> java.lang.CharSequence), &[])?.l()?;
        if chars.is_null() { return Ok(None); }
        let string = env.call_method(&chars, jni_str!("toString"), jni_sig!(() -> java.lang.String), &[])?.l()?;
        Ok(Some(env.cast_local::<JString>(string)?.try_to_string(env)?))
    })
}
