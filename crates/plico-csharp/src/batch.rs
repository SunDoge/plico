use crate::abi::{Batch, Command};
use std::collections::HashSet;

pub const ROOT: u32 = u32::MAX;
pub const COLUMN: u32 = 1;
pub const ROW: u32 = 2;
pub const TEXT: u32 = 3;
pub const NODE: u32 = 16;
pub const NUMERIC_INPUT: u32 = 19;
pub const INPUT: u32 = 7;
pub const IMAGE: u32 = 8;
pub const SHADER: u32 = 10;
pub const SVG: u32 = 11;
pub const CANVAS: u32 = 13;
pub const PORTAL: u32 = 17;
pub const PANEL: u32 = 18;
pub const VIRTUAL_LIST: u32 = 15;
pub const WIDTH_FILL: u32 = 1;
pub const HEIGHT_FILL: u32 = 2;
pub const BACKGROUND: u32 = 4;
pub const FOREGROUND: u32 = 8;
pub const DISABLED: u32 = 64;
pub const SCROLL: u32 = 128;
const FLAGS: u32 = (1 << 27) - 1;
const MAX_COMMANDS: usize = 100_000;
const MAX_BYTES: usize = 16 * 1024 * 1024;

pub struct OwnedBatch {
    pub properties: std::collections::HashMap<u64, crate::properties::Properties>,
    pub animate: bool,
    pub lists: std::collections::HashMap<u64, crate::virtual_list::Description>,
    pub dark: bool,
    pub commands: Vec<Command>,
    pub bytes: Vec<u8>,
    pub children: Vec<Vec<usize>>,
    pub roots: Vec<usize>,
}
impl OwnedBatch {
    /// Copy all foreign buffers before returning; retain no managed pointer.
    pub unsafe fn copy(batch: Batch) -> Result<Self, String> {
        if batch.version != 5 || batch.dark > 1 || batch.animate > 1 {
            return Err("unsupported batch version".into());
        }
        let commands =
            unsafe { super::slice(batch.commands, batch.command_count as usize, MAX_COMMANDS)? }
                .to_vec();
        let bytes =
            unsafe { super::slice(batch.utf8, batch.utf8_len as usize, MAX_BYTES)? }.to_vec();
        let mut result = Self::validate(commands, bytes)?;
        result.dark = batch.dark != 0;
        result.animate = batch.animate != 0;
        Ok(result)
    }
    pub fn validate(commands: Vec<Command>, bytes: Vec<u8>) -> Result<Self, String> {
        if commands.len() > MAX_COMMANDS || bytes.len() > MAX_BYTES {
            return Err("batch exceeds resource budget".into());
        }
        std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
        let mut properties = std::collections::HashMap::new();
        let mut lists = std::collections::HashMap::new();
        let mut children = vec![Vec::new(); commands.len()];
        let mut roots = Vec::new();
        let mut ids = HashSet::with_capacity(commands.len());
        let mut depths = vec![0; commands.len()];
        for (i, command) in commands.iter().enumerate() {
            if command.id == 0 || !ids.insert(command.id) {
                return Err(format!("duplicate/zero widget id {}", command.id));
            }
            if (!(COLUMN..=VIRTUAL_LIST).contains(&command.kind)
                && !matches!(command.kind, NODE | PORTAL | PANEL | NUMERIC_INPUT))
                || matches!(command.kind, 4 | 5 | 6 | 9 | 12 | 14)
                || command.flags & !FLAGS != 0
            {
                return Err("unknown command or flags".into());
            }
            if command.parent == ROOT {
                roots.push(i);
            } else {
                let parent = command.parent as usize;
                if parent >= i
                    || !matches!(
                        commands[parent].kind,
                        COLUMN | ROW | VIRTUAL_LIST | NODE | PORTAL | PANEL
                    )
                {
                    return Err("parent must be an earlier container".into());
                }
                depths[i] = depths[parent] + 1;
                if depths[i] > 64 {
                    return Err("layout depth exceeds 64".into());
                }
                children[parent].push(i);
            }
            for (start, len) in [
                (command.text_start, command.text_len),
                (command.value_start, command.value_len),
                (command.properties_start, command.properties_len),
            ] {
                let end = (start as usize)
                    .checked_add(len as usize)
                    .ok_or("text span overflow")?;
                let part = bytes
                    .get(start as usize..end)
                    .ok_or("text span out of bounds")?;
                std::str::from_utf8(part).map_err(|_| "text span splits UTF-8")?;
            }
            if command.properties_len > 0 {
                let part = |start: u32, len: u32| {
                    std::str::from_utf8(&bytes[start as usize..(start + len) as usize]).unwrap()
                };
                properties.insert(
                    command.id,
                    crate::properties::Properties::parse(
                        part(command.properties_start, command.properties_len),
                        part(command.text_start, command.text_len),
                    )?,
                );
            }
            if command.kind == INPUT && command.flags & (3 << 18) == 3 << 18 {
                return Err("password inputs cannot be multiline".into());
            }
            if command.kind == VIRTUAL_LIST {
                let start = command.value_start as usize;
                let end = start + command.value_len as usize;
                lists.insert(
                    command.id,
                    crate::virtual_list::Description::parse(
                        std::str::from_utf8(&bytes[start..end]).unwrap(),
                    )?,
                );
            }
            if ![command.width, command.height]
                .iter()
                .all(|v| v.is_finite() && *v >= -1. && *v <= 16384.)
                || ![command.grow, command.gap]
                    .iter()
                    .all(|v| v.is_finite() && *v >= 0. && *v <= 16384.)
                || ![command.padding, command.radius]
                    .iter()
                    .all(|v| v.is_finite() && *v >= -1. && *v <= 16384.)
                || command.font_size > 1024
                || command.weight > 1000
                || command.semantic_role > 6
                || !command.opacity.is_finite()
                || !(0. ..=1.).contains(&command.opacity)
                || command.background > 0xffffff
                || command.foreground > 0xffffff
                || command.hover_background > 0xffffff
                || command.pressed_background > 0xffffff
                || command.border_color > 0xffffff
                || ![
                    command.border_width,
                    command.shrink,
                    command.min_width,
                    command.min_height,
                ]
                .iter()
                .all(|v| v.is_finite() && *v >= 0. && *v <= 16384.)
            {
                return Err("invalid style value".into());
            }
            if command.flags & (1 << 16) != 0 && !(0. ..=100.).contains(&command.width) {
                return Err("invalid percentage width".into());
            }
            if command.kind == SHADER && command.text_len > 64 * 1024 {
                return Err("shader source exceeds 64 KiB".into());
            }
            if matches!(command.kind, NUMERIC_INPUT)
                && ![command.value, command.min, command.max]
                    .iter()
                    .all(|v| v.is_finite())
            {
                return Err("nonfinite control value".into());
            }
            if command.kind == NUMERIC_INPUT
                && (command.min > command.max || !(command.max - command.min).is_finite())
            {
                return Err("invalid slider bounds".into());
            }
        }
        for (i, command) in commands.iter().enumerate() {
            if let Some(list) = lists.get(&command.id)
                && children[i].len() != list.end - list.start
            {
                return Err("virtual list requires one child per declared row".into());
            }
        }
        Ok(Self {
            properties,
            animate: false,
            lists,
            dark: false,
            commands,
            bytes,
            children,
            roots,
        })
    }
    pub fn text(&self, start: u32, len: u32) -> &str {
        // All spans and boundaries were validated before adoption.
        std::str::from_utf8(&self.bytes[start as usize..start as usize + len as usize]).unwrap()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn command(id: u64, parent: u32, kind: u32) -> Command {
        Command {
            id,
            parent,
            kind,
            width: -1.,
            height: -1.,
            ..Default::default()
        }
    }
    #[test]
    fn validates_ids_parents_utf8_and_style_before_adoption() {
        assert!(
            OwnedBatch::validate(vec![command(1, ROOT, COLUMN), command(2, 0, TEXT)], vec![])
                .is_ok()
        );
        assert!(
            OwnedBatch::validate(vec![command(1, ROOT, COLUMN), command(1, 0, TEXT)], vec![])
                .is_err()
        );
        assert!(OwnedBatch::validate(vec![command(1, 0, COLUMN)], vec![]).is_err());
        let mut c = command(1, ROOT, TEXT);
        c.text_len = 1;
        assert!(OwnedBatch::validate(vec![c], "中".as_bytes().to_vec()).is_err());
        c.text_len = 0;
        c.gap = f32::NAN;
        assert!(OwnedBatch::validate(vec![c], vec![]).is_err());
    }
    #[test]
    fn rejects_removed_widget_opcodes() {
        for kind in [4, 5, 6, 9, 12, 14] {
            assert!(OwnedBatch::validate(vec![command(1, ROOT, kind)], vec![]).is_err());
        }
        assert!(
            OwnedBatch::validate(vec![command(1, ROOT, NODE), command(2, 0, TEXT)], vec![]).is_ok()
        );
    }
    #[test]
    fn rejects_deep_trees_and_leaf_parents() {
        let nodes = (0..66)
            .map(|i| command(i + 1, if i == 0 { ROOT } else { (i - 1) as u32 }, COLUMN))
            .collect();
        assert!(OwnedBatch::validate(nodes, vec![]).is_err());
        assert!(
            OwnedBatch::validate(vec![command(1, ROOT, TEXT), command(2, 0, TEXT)], vec![])
                .is_err()
        );
    }
}

#[cfg(test)]
mod ownership_tests {
    use super::*;
    #[test]
    fn copies_foreign_storage_and_rejects_null_nonempty_buffers() {
        let mut bytes = b"hello".to_vec();
        let mut commands = [Command {
            id: 1,
            parent: ROOT,
            kind: TEXT,
            text_len: 5,
            width: -1.,
            height: -1.,
            ..Default::default()
        }];
        let batch = Batch {
            version: 5,
            dark: 0,
            animate: 0,
            commands: commands.as_ptr(),
            command_count: 1,
            utf8: bytes.as_ptr(),
            utf8_len: 5,
        };
        let owned = unsafe { OwnedBatch::copy(batch) }.unwrap();
        bytes[0] = b'X';
        commands[0].text_len = 1;
        assert_eq!(commands[0].text_len, 1);
        assert_eq!(owned.text(0, 5), "hello");
        assert_eq!(owned.commands[0].text_len, 5);
        assert!(
            unsafe {
                OwnedBatch::copy(Batch {
                    commands: std::ptr::null(),
                    ..batch
                })
            }
            .is_err()
        );
    }
}

impl Drop for OwnedBatch {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.bytes);
    }
}

#[cfg(test)]
mod property_channel_tests {
    use super::*;
    #[test]
    fn input_value_and_properties_are_independent_owned_spans() {
        let label = "Field";
        let value = "你好";
        let json = r#"{"max_width":320,"padding_x":12,"placeholder":"Type here"}"#;
        let bytes = format!("{label}{value}{json}").into_bytes();
        let command = Command {
            id: 1,
            parent: ROOT,
            kind: INPUT,
            text_len: label.len() as u32,
            value_start: label.len() as u32,
            value_len: value.len() as u32,
            properties_start: (label.len() + value.len()) as u32,
            properties_len: json.len() as u32,
            opacity: 1.,
            ..Default::default()
        };
        let batch = Batch {
            version: 5,
            dark: 0,
            animate: 0,
            commands: &command,
            command_count: 1,
            utf8: bytes.as_ptr(),
            utf8_len: bytes.len() as u32,
        };
        let owned = unsafe { OwnedBatch::copy(batch) }.unwrap();
        assert_eq!(owned.text(command.value_start, command.value_len), value);
        assert_eq!(owned.properties[&1].max_width, Some(320.));
        let invalid = Command {
            properties_start: 6,
            ..command
        };
        assert!(
            unsafe {
                OwnedBatch::copy(Batch {
                    commands: &invalid,
                    ..batch
                })
            }
            .is_err()
        );
    }
}
