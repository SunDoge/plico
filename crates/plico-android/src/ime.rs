use super::AndroidApp;
use jni::{
    JavaVM, jni_sig, jni_str,
    objects::{JClass, JObject, JString, JValue},
    refs::Global,
};
use plico_renderer::{ImeEvent, ImeState, WidgetImePurpose};
use std::collections::HashMap;
use std::sync::{
    Arc, LazyLock, Mutex,
    atomic::{AtomicI64, Ordering},
    mpsc,
};

type Destination = (mpsc::Sender<ImeEvent>, Arc<dyn Fn() + Send + Sync>);
static CLIENTS: LazyLock<Mutex<HashMap<i64, Destination>>> = LazyLock::new(Default::default);
static NEXT: AtomicI64 = AtomicI64::new(1);

#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_plico_PlicoActivity_edit(
    mut env: jni::EnvUnowned<'_>,
    _: JClass<'_>,
    token: i64,
    kind: i32,
    value: JString<'_>,
    before: i32,
    after: i32,
) {
    env.with_env(|env| -> jni::errors::Result<()> {
        let event = match kind {
            0 => ImeEvent::Preedit {
                text: value.try_to_string(env)?,
                cursor: None,
            },
            1 => ImeEvent::Commit(value.try_to_string(env)?),
            2 => ImeEvent::DeleteSurrounding {
                before_bytes: before.max(0) as usize,
                after_bytes: after.max(0) as usize,
            },
            _ => return Ok(()),
        };
        if let Some((tx, wake)) = CLIENTS.lock().unwrap().get(&token) {
            if tx.send(event).is_ok() {
                wake();
            }
        }
        Ok(())
    })
    .resolve::<jni::errors::ThrowRuntimeExAndDefault>()
}

pub(super) struct Editor {
    session: Option<(u64, i64, mpsc::Receiver<ImeEvent>)>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl Editor {
    pub fn new(wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self {
            session: None,
            wake,
        }
    }
    pub fn drain(&self) -> impl Iterator<Item = ImeEvent> + '_ {
        self.session.iter().flat_map(|(_, _, rx)| rx.try_iter())
    }
    fn close(&mut self) {
        if let Some((_, token, _)) = self.session.take() {
            CLIENTS.lock().unwrap().remove(&token);
        }
    }
    pub fn update(&mut self, app: &AndroidApp, state: Option<ImeState>) -> jni::errors::Result<()> {
        if self.session.as_ref().map(|s| s.0) == state.as_ref().map(|s| s.client_id) {
            return Ok(());
        }
        self.close();
        let mut token = 0;
        let mut surrounding = "";
        let mut cursor = 0;
        let mut anchor = 0;
        let mut ty = 1;
        if let Some(s) = &state {
            token = NEXT.fetch_add(1, Ordering::Relaxed);
            let (tx, rx) = mpsc::channel();
            CLIENTS
                .lock()
                .unwrap()
                .insert(token, (tx, self.wake.clone()));
            self.session = Some((s.client_id, token, rx));
            use WidgetImePurpose::*;
            ty = match s.purpose {
                Password => 129,
                Pin => 18,
                Terminal => 0x80001,
                Number => 0x3002,
                Phone => 3,
                Url => 17,
                Email => 33,
                Date => 20,
                Time => 36,
                Normal => 1,
                _ => 4,
            };
            if s.multiline {
                ty |= 0x20000;
            }
            if !matches!(s.purpose, Password | Pin) {
                surrounding = &s.surrounding_text;
                let offset = |i| surrounding.get(..i).unwrap_or("").encode_utf16().count() as i32;
                cursor = offset(s.surrounding_cursor);
                anchor = offset(s.surrounding_anchor);
            }
        }
        JavaVM::singleton()?.attach_current_thread(|env| {
            let raw = app.activity_as_ptr().cast();
            // SAFETY: the Activity owns this global reference throughout this call.
            let activity = unsafe { env.as_cast_raw::<Global<JObject>>(&raw)? };
            let text = env.new_string(surrounding)?;
            let result = env.call_method(
                activity,
                jni_str!("plicoEditor"),
                jni_sig!((long,java.lang.String,int,int,int)->void),
                &[
                    JValue::Long(token),
                    JValue::Object(&text),
                    JValue::Int(cursor),
                    JValue::Int(anchor),
                    JValue::Int(ty),
                ],
            );
            if result.is_err() && env.exception_check() {
                env.exception_clear();
            }
            result.map(|_| ())
        })
    }
}
impl Drop for Editor {
    fn drop(&mut self) {
        self.close();
    }
}
