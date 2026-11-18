use super::AndroidApp;
use jni::{
    JavaVM, jni_sig, jni_str,
    objects::{JObject, JValue},
    refs::Global,
};

pub(super) fn open(app: &AndroidApp, url: &str) -> jni::errors::Result<()> {
    JavaVM::singleton()?.attach_current_thread(|env| {
        let raw = app.activity_as_ptr().cast();
        // SAFETY: app keeps the borrowed global Activity reference alive.
        let activity = unsafe { env.as_cast_raw::<Global<JObject>>(&raw)? };
        let url = env.new_string(url)?;
        let uri = env
            .call_static_method(
                jni_str!("android/net/Uri"),
                jni_str!("parse"),
                jni_sig!((java.lang.String) -> android.net.Uri),
                &[JValue::Object(&url)],
            )?
            .l()?;
        let action = env.new_string("android.intent.action.VIEW")?;
        let intent = env.new_object(
            jni_str!("android/content/Intent"),
            jni_sig!((java.lang.String, android.net.Uri) -> void),
            &[JValue::Object(&action), JValue::Object(&uri)],
        )?;
        env.call_method(
            activity,
            jni_str!("startActivity"),
            jni_sig!((android.content.Intent) -> void),
            &[JValue::Object(&intent)],
        )?;
        Ok(())
    })
}
