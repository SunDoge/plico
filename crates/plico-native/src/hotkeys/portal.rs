use super::*;
use ashpd::desktop::global_shortcuts::{GlobalShortcuts as Proxy, NewShortcut};
use futures_util::StreamExt;
use std::time::Duration;
use tokio::sync::{mpsc as channel, oneshot};
enum Command {
    Register(Binding, u64),
    Unregister(String),
}
pub(super) struct Portal {
    commands: channel::UnboundedSender<Command>,
}
impl Portal {
    pub fn new(sender: Sender) -> Result<Self, Error> {
        let (commands, mut receiver) = channel::unbounded_channel();
        // Construct the runtime before spawning so construction failures are synchronous.
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| Error::new(e.to_string()))?;
        std::thread::Builder::new()
            .name("plico-hotkeys".into())
            .spawn(move || {
                runtime.block_on(async move {
                    let mut tasks = tokio::task::JoinSet::new();
                    let mut stops = BTreeMap::<String, oneshot::Sender<()>>::new();
                    while let Some(command) = receiver.recv().await {
                        match command {
                            Command::Register(binding, generation) => {
                                let (stop, cancel) = oneshot::channel();
                                stops.insert(binding.id.clone(), stop);
                                let sender = sender.clone();
                                tasks.spawn(async move {
                                    run(binding, generation, sender, cancel).await;
                                });
                            }
                            Command::Unregister(id) => {
                                stops.remove(&id);
                            }
                        }
                        while tasks.try_join_next().is_some() {}
                    }
                    drop(stops); // Cancellation closes each portal session before its task exits.
                    while tasks.join_next().await.is_some() {}
                })
            })
            .map_err(|e| Error::new(e.to_string()))?;
        Ok(Self { commands })
    }
    pub fn register(&mut self, binding: Binding, generation: u64) -> Result<(), Error> {
        self.commands
            .send(Command::Register(binding, generation))
            .map_err(|_| Error::new("portal worker stopped"))
    }
    pub fn unregister(&mut self, id: &str) -> Result<(), Error> {
        self.commands
            .send(Command::Unregister(id.into()))
            .map_err(|_| Error::new("portal worker stopped"))
    }
}
fn preferred(hotkey: HotKey) -> Result<String, Error> {
    use global_hotkey::hotkey::{Code, Modifiers};
    let mut trigger = String::new();
    for (modifier, name) in [
        (Modifiers::CONTROL, "CTRL+"),
        (Modifiers::ALT, "ALT+"),
        (Modifiers::SHIFT, "SHIFT+"),
        (Modifiers::META, "LOGO+"),
    ] {
        if hotkey.mods.contains(modifier) {
            trigger.push_str(name);
        }
    }
    let code = hotkey.key.to_string();
    let key = match hotkey.key {
        Code::Space => "space",
        Code::Enter => "Return",
        Code::Escape => "Escape",
        Code::Backspace => "BackSpace",
        Code::Tab => "Tab",
        Code::ArrowUp => "Up",
        Code::ArrowDown => "Down",
        Code::ArrowLeft => "Left",
        Code::ArrowRight => "Right",
        Code::Delete => "Delete",
        Code::Home => "Home",
        Code::End => "End",
        Code::PageUp => "Prior",
        Code::PageDown => "Next",
        _ if code.starts_with("Key") => &code[3..],
        _ if code.starts_with("Digit") => &code[5..],
        _ if code.starts_with('F') && code[1..].parse::<u8>().is_ok() => &code,
        _ => return Err(Error::new(format!("no portal trigger mapping for {code}"))),
    };
    trigger.push_str(key);
    Ok(trigger)
}
async fn run(binding: Binding, generation: u64, sender: Sender, mut cancel: oneshot::Receiver<()>) {
    let fail = |message: String| {
        sender.send(
            generation,
            HotkeyEvent::Failed {
                id: binding.id.clone(),
                message,
            },
        )
    };
    let trigger = match preferred(binding.hotkey) {
        Ok(t) => t,
        Err(e) => {
            fail(e.to_string());
            return;
        }
    };
    let proxy = match tokio::time::timeout(Duration::from_secs(10), Proxy::new()).await {
        Ok(Ok(p)) => p,
        other => {
            fail(format!("GlobalShortcuts portal unavailable: {other:?}"));
            return;
        }
    };
    let session = match tokio::time::timeout(
        Duration::from_secs(10),
        proxy.create_session(Default::default()),
    )
    .await
    {
        Ok(Ok(s)) => s,
        other => {
            fail(format!("portal session failed: {other:?}"));
            return;
        }
    };
    let operation = async {
        let mut activated = proxy.receive_activated().await.map_err(|e| e.to_string())?;
        let mut released = proxy
            .receive_deactivated()
            .await
            .map_err(|e| e.to_string())?;
        let mut changed = proxy
            .receive_shortcuts_changed()
            .await
            .map_err(|e| e.to_string())?;
        let mut closed = session.receive_closed().await.map_err(|e| e.to_string())?;
        let shortcuts = [NewShortcut::new(&binding.id, &binding.description)
            .preferred_trigger(Some(trigger.as_str()))];
        let response = proxy
            .bind_shortcuts(&session, &shortcuts, None, Default::default())
            .await
            .map_err(|e| e.to_string())?;
        let response = response.response().map_err(|e| e.to_string())?;
        let actual = response
            .shortcuts()
            .iter()
            .find(|s| s.id() == binding.id)
            .ok_or("desktop did not bind the requested action")?;
        sender.send(
            generation,
            HotkeyEvent::Registered {
                id: binding.id.clone(),
                trigger: actual.trigger_description().into(),
            },
        );
        let session_path = serde_json::to_value(&session).map_err(|e| e.to_string())?;
        let mut pressed = false;
        loop {
            tokio::select! {
                event=activated.next()=>{
                    let event=event.ok_or("portal activation stream ended")?;
                    if !pressed && event.shortcut_id()==binding.id && session_path.as_str()==Some(event.session_handle().as_str()) {
                        pressed=true;
                        let token=event.options().get("activation_token").and_then(|v|v.downcast_ref::<&str>().ok()).map(str::to_owned);
                        sender.send(generation,HotkeyEvent::Pressed{id:binding.id.clone(),activation_token:token});
                    }
                }
                event=released.next()=>{
                    let event=event.ok_or("portal release stream ended")?;
                    if pressed && event.shortcut_id()==binding.id && session_path.as_str()==Some(event.session_handle().as_str()) {
                        pressed=false;
                        sender.send(generation,HotkeyEvent::Released{id:binding.id.clone()});
                    }
                }
                event=changed.next()=>{
                    let event=event.ok_or("portal shortcut-change stream ended")?;
                    if session_path.as_str()==Some(event.session_handle().as_str()) {
                        let shortcut=event.shortcuts().iter().find(|s|s.id()==binding.id).ok_or("desktop removed the shortcut")?;
                        sender.send(generation,HotkeyEvent::Registered{id:binding.id.clone(),trigger:shortcut.trigger_description().into()});
                    }
                }
                _=closed.next()=>return Err::<(),String>("desktop closed the shortcut session".into()),
            }
        }
    };
    tokio::select! { biased;
        _=&mut cancel=>{},
        result=operation=>if let Err(message)=result {fail(message);},
    }
    let _ = tokio::time::timeout(Duration::from_secs(1), session.close()).await;
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maps_native_codes_to_portal_triggers() {
        assert_eq!(
            preferred("Ctrl+Shift+K".parse().unwrap()).unwrap(),
            "CTRL+SHIFT+K"
        );
        assert_eq!(
            preferred("Alt+Space".parse().unwrap()).unwrap(),
            "ALT+space"
        );
    }
}
