use std::sync::Arc;

use ksni::blocking::TrayMethods;
use ksni::menu::{MenuItem, StandardItem};
use ksni::{ToolTip, Tray};
use qmetaobject::QPointer;

use crate::AppBridge;

const SHOW: i32 = 0;
const TOGGLE: i32 = 1;
const PREVIOUS: i32 = 2;
const NEXT: i32 = 3;
const QUIT: i32 = 4;

#[allow(missing_debug_implementations)]
pub(crate) struct TrayHandle {
    inner: ksni::blocking::Handle<QingyinTray>,
}

impl TrayHandle {
    pub(crate) fn update(&self, playing: bool, tooltip: String) {
        self.inner.update(|tray| {
            tray.playing = playing;
            tray.tooltip = tooltip;
        });
    }
}

pub(crate) fn spawn(bridge: QPointer<AppBridge>) -> Result<TrayHandle, ksni::Error> {
    let dispatch = Arc::new(qmetaobject::queued_callback(move |action: i32| {
        let Some(bridge) = bridge.as_pinned() else {
            return;
        };
        let bridge = bridge.borrow_mut();
        match action {
            SHOW => bridge.tray_show_requested(),
            TOGGLE => bridge.playback.borrow_mut().toggle_playback_internal(),
            PREVIOUS => bridge.playback.borrow_mut().play_previous_internal(),
            NEXT => bridge.playback.borrow_mut().play_next_internal(),
            QUIT => bridge.tray_quit_requested(),
            _ => {}
        }
    }));
    let handle = QingyinTray {
        dispatch,
        playing: false,
        tooltip: "清音".into(),
    }
    .spawn()?;
    Ok(TrayHandle { inner: handle })
}

#[allow(missing_debug_implementations)]
struct QingyinTray {
    dispatch: Arc<dyn Fn(i32) + Send + Sync>,
    playing: bool,
    tooltip: String,
}

impl Tray for QingyinTray {
    fn id(&self) -> String {
        "qingyin".into()
    }

    fn icon_name(&self) -> String {
        "audio-x-generic".into()
    }

    fn title(&self) -> String {
        "清音".into()
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            icon_name: "audio-x-generic".into(),
            title: self.tooltip.clone(),
            ..ToolTip::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        (self.dispatch)(SHOW);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            item("显示主窗口", SHOW, &self.dispatch),
            item(
                if self.playing { "暂停" } else { "播放" },
                TOGGLE,
                &self.dispatch,
            ),
            item("上一首", PREVIOUS, &self.dispatch),
            item("下一首", NEXT, &self.dispatch),
            item("退出", QUIT, &self.dispatch),
        ]
    }
}

fn item(
    label: &str,
    action: i32,
    dispatch: &Arc<dyn Fn(i32) + Send + Sync>,
) -> MenuItem<QingyinTray> {
    let dispatch = Arc::clone(dispatch);
    StandardItem {
        label: label.into(),
        activate: Box::new(move |_| dispatch(action)),
        ..StandardItem::default()
    }
    .into()
}
