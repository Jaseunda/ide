use gpui::{
    AnimationExt, AnyElement, App, Bounds, Context, Entity, EventEmitter, FocusHandle, Focusable,
    Pixels, Render, SharedString, WeakEntity, Window, canvas, div, prelude::*, px,
};
use menu::Confirm;
use raw_window_handle::HasWindowHandle;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use theme::ActiveTheme;
use ui::{
    ButtonCommon, Clickable, Icon, IconButton, IconName, Label, LabelCommon, Tooltip, h_flex,
    v_flex,
};
use ui_input::InputField;

use crate::Workspace;
use crate::item::{Item, ItemEvent, TabContentParams};

#[cfg(target_os = "macos")]
mod mac_web_view {
    use super::*;
    use block2::RcBlock;
    use cocoa::base::{NO, YES, id, nil};
    use cocoa::foundation::{NSPoint, NSRect, NSSize, NSString, NSURL};
    use objc::runtime::Class;
    use objc::{msg_send, sel, sel_impl};
    use objc2::runtime::AnyObject;
    use parking_lot::Mutex;

    pub struct NativeWebView {
        pub view: id,
        pub parent_view: id,
        latest_snapshot: Arc<Mutex<Option<Arc<gpui::Image>>>>,
        is_taking_snapshot: Arc<AtomicBool>,
        snapshot_version: Arc<AtomicUsize>,
    }

    unsafe impl Send for NativeWebView {}
    unsafe impl Sync for NativeWebView {}

    impl NativeWebView {
        pub fn new(window: &mut Window, initial_url: &str) -> Option<Self> {
            let parent_view: id = if let Ok(handle) = window.window_handle() {
                if let raw_window_handle::RawWindowHandle::AppKit(handle) = handle.as_raw() {
                    handle.ns_view.as_ptr() as id
                } else {
                    nil
                }
            } else {
                nil
            };

            if parent_view.is_null() {
                return None;
            }

            unsafe {
                let wk_config_cls = Class::get("WKWebViewConfiguration")?;
                let config: id = msg_send![wk_config_cls, new];

                let wk_view_cls = Class::get("WKWebView")?;
                let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(100.0, 100.0));
                let webview: id = msg_send![wk_view_cls, alloc];
                let webview: id = msg_send![webview, initWithFrame: frame configuration: config];

                if webview.is_null() {
                    return None;
                }

                let _: () = msg_send![parent_view, addSubview: webview];

                let mut this = Self {
                    view: webview,
                    parent_view,
                    latest_snapshot: Arc::new(Mutex::new(None)),
                    is_taking_snapshot: Arc::new(AtomicBool::new(false)),
                    snapshot_version: Arc::new(AtomicUsize::new(0)),
                };

                this.load_url(initial_url);
                Some(this)
            }
        }

        pub fn get_snapshot(&self) -> Option<Arc<gpui::Image>> {
            self.latest_snapshot.lock().clone()
        }

        pub fn has_no_snapshot(&self) -> bool {
            self.latest_snapshot.lock().is_none()
        }

        pub fn get_snapshot_version(&self) -> usize {
            self.snapshot_version.load(Ordering::SeqCst)
        }

        pub fn is_loading(&self) -> bool {
            unsafe {
                let loading: cocoa::base::BOOL = msg_send![self.view, isLoading];
                loading == YES
            }
        }

        pub fn estimated_progress(&self) -> f64 {
            unsafe {
                let progress: f64 = msg_send![self.view, estimatedProgress];
                progress
            }
        }

        pub fn take_snapshot(&self) {
            if self.is_hidden() {
                return;
            }

            if self
                .is_taking_snapshot
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
            {
                return;
            }

            let snapshot_store = self.latest_snapshot.clone();
            let flag = self.is_taking_snapshot.clone();
            let ver = self.snapshot_version.clone();

            let handler = RcBlock::new(move |image: *mut AnyObject, _error: *mut AnyObject| {
                flag.store(false, Ordering::SeqCst);
                if image.is_null() {
                    return;
                }

                unsafe {
                    let image = image as id;
                    let tiff: id = msg_send![image, TIFFRepresentation];
                    if tiff.is_null() {
                        return;
                    }

                    let rep_cls = Class::get("NSBitmapImageRep").unwrap();
                    let rep: id = msg_send![rep_cls, imageRepWithData: tiff];
                    if rep.is_null() {
                        return;
                    }

                    let num_cls = Class::get("NSNumber").unwrap();
                    let factor: id = msg_send![num_cls, numberWithDouble: 0.85f64];
                    let dict_cls = Class::get("NSDictionary").unwrap();
                    let str_cls = Class::get("NSString").unwrap();
                    let key: id = msg_send![str_cls, stringWithUTF8String: b"NSImageCompressionFactor\0".as_ptr()];
                    let props: id = msg_send![dict_cls, dictionaryWithObject: factor forKey: key];

                    // 3usize is NSBitmapImageFileTypeJPEG / NSJPEGFileType
                    let jpeg_data: id = msg_send![rep, representationUsingType: 3usize properties: props];
                    if !jpeg_data.is_null() {
                        let length: usize = msg_send![jpeg_data, length];
                        let bytes: *const u8 = msg_send![jpeg_data, bytes];
                        if length > 0 && !bytes.is_null() {
                            let slice = std::slice::from_raw_parts(bytes, length);
                            let img = Arc::new(gpui::Image::from_bytes(gpui::ImageFormat::Jpeg, slice.to_vec()));
                            *snapshot_store.lock() = Some(img);
                            ver.fetch_add(1, Ordering::SeqCst);
                        }
                    }
                }
            });

            unsafe {
                let config_cls = Class::get("WKSnapshotConfiguration").unwrap();
                let config: id = msg_send![config_cls, alloc];
                let config: id = msg_send![config, init];

                let num_cls = Class::get("NSNumber").unwrap();
                let width_num: id = msg_send![num_cls, numberWithDouble: 1280.0f64];
                let _: () = msg_send![config, setSnapshotWidth: width_num];

                let block_ptr = RcBlock::as_ptr(&handler) as id;
                let retained_block: id = msg_send![block_ptr, copy];

                let _: () = msg_send![
                    self.view,
                    takeSnapshotWithConfiguration: config
                    completionHandler: retained_block
                ];
                let _: () = msg_send![config, autorelease];
                let _: () = msg_send![retained_block, release];
            }
        }

        pub fn load_url(&mut self, url: &str) {
            unsafe {
                let trimmed = url.trim();
                let full_url = if trimmed.starts_with("http://")
                    || trimmed.starts_with("https://")
                    || trimmed.starts_with("file://")
                    || trimmed.starts_with("about:")
                {
                    trimmed.to_string()
                } else if trimmed.starts_with("localhost") || trimmed.starts_with("127.0.0.1") {
                    format!("http://{}", trimmed)
                } else {
                    format!("https://{}", trimmed)
                };

                let ns_str = NSString::alloc(nil).init_str(&full_url);
                let ns_url = NSURL::URLWithString_(nil, ns_str);
                if !ns_url.is_null() {
                    let req_cls = Class::get("NSURLRequest").unwrap();
                    let req: id = msg_send![req_cls, requestWithURL: ns_url];
                    let _: id = msg_send![self.view, loadRequest: req];
                }
            }
        }

        pub fn get_title(&self) -> Option<String> {
            unsafe {
                let title: id = msg_send![self.view, title];
                if title.is_null() {
                    return None;
                }
                let length: usize = msg_send![title, length];
                if length == 0 {
                    return None;
                }
                let utf8_ptr: *const std::ffi::c_char = msg_send![title, UTF8String];
                if utf8_ptr.is_null() {
                    return None;
                }
                let c_str = std::ffi::CStr::from_ptr(utf8_ptr);
                let s = c_str.to_string_lossy().trim().to_string();
                if s.is_empty() { None } else { Some(s) }
            }
        }

        pub fn get_url(&self) -> Option<String> {
            unsafe {
                let ns_url: id = msg_send![self.view, URL];
                if ns_url.is_null() {
                    return None;
                }
                let abs_str: id = msg_send![ns_url, absoluteString];
                if abs_str.is_null() {
                    return None;
                }
                let utf8_ptr: *const std::ffi::c_char = msg_send![abs_str, UTF8String];
                if utf8_ptr.is_null() {
                    return None;
                }
                let c_str = std::ffi::CStr::from_ptr(utf8_ptr);
                let s = c_str.to_string_lossy().trim().to_string();
                if s.is_empty() { None } else { Some(s) }
            }
        }

        pub fn go_back(&self) {
            unsafe {
                let can_go_back: cocoa::base::BOOL = msg_send![self.view, canGoBack];
                if can_go_back == YES {
                    let _: id = msg_send![self.view, goBack];
                }
            }
        }

        pub fn go_forward(&self) {
            unsafe {
                let can_go_forward: cocoa::base::BOOL = msg_send![self.view, canGoForward];
                if can_go_forward == YES {
                    let _: id = msg_send![self.view, goForward];
                }
            }
        }

        pub fn reload(&self) {
            unsafe {
                let _: id = msg_send![self.view, reload];
            }
        }

        pub fn is_hidden(&self) -> bool {
            unsafe {
                let is_hidden: cocoa::base::BOOL = msg_send![self.view, isHidden];
                is_hidden == YES
            }
        }

        pub fn set_hidden(&self, hidden: bool) {
            unsafe {
                let current: cocoa::base::BOOL = msg_send![self.view, isHidden];
                let target = if hidden { YES } else { NO };
                if current != target {
                    let _: () = msg_send![self.view, setHidden: target];
                }
            }
        }

        pub fn update_frame(&self, bounds: Bounds<Pixels>) {
            unsafe {
                let parent_frame: NSRect = msg_send![self.parent_view, bounds];
                let parent_h = parent_frame.size.height;

                let x = bounds.origin.x.as_f32() as f64;
                let w = bounds.size.width.as_f32().max(1.0) as f64;
                let h = bounds.size.height.as_f32().max(1.0) as f64;
                let y = (parent_h - (bounds.origin.y.as_f32() as f64 + h)).max(0.0);

                let current_frame: NSRect = msg_send![self.view, frame];
                if (current_frame.origin.x - x).abs() > 0.5
                    || (current_frame.origin.y - y).abs() > 0.5
                    || (current_frame.size.width - w).abs() > 0.5
                    || (current_frame.size.height - h).abs() > 0.5
                {
                    let target_frame = NSRect::new(NSPoint::new(x, y), NSSize::new(w, h));
                    let _: () = msg_send![self.view, setFrame: target_frame];
                    if w > 100.0 && h > 100.0 && self.has_no_snapshot() {
                        self.take_snapshot();
                    }
                }
            }
        }
    }

    impl Drop for NativeWebView {
        fn drop(&mut self) {
            unsafe {
                if !self.view.is_null() {
                    let _: () = msg_send![self.view, removeFromSuperview];
                    let _: () = msg_send![self.view, release];
                    self.view = nil;
                }
            }
        }
    }
}

pub fn init(cx: &mut App) {
    cx.bind_keys([
        gpui::KeyBinding::new("alt-b", zed_actions::OpenBrowserTab, None),
        gpui::KeyBinding::new("cmd-alt-b", zed_actions::OpenBrowserTab, None),
        gpui::KeyBinding::new("cmd-k b", zed_actions::OpenBrowserTab, None),
    ]);
}

pub fn open_browser_tab(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let workspace_handle = cx.entity().downgrade();
    let browser =
        cx.new(|cx| BrowserTab::new("http://localhost:3000", Some(workspace_handle), window, cx));
    let active_pane = workspace.active_pane().clone();
    workspace.add_item(active_pane, Box::new(browser), None, true, true, window, cx);
}

pub struct BrowserTab {
    focus_handle: FocusHandle,
    url: String,
    title: String,
    url_input: Entity<InputField>,
    workspace: Option<WeakEntity<Workspace>>,
    #[cfg(target_os = "macos")]
    web_view: Option<mac_web_view::NativeWebView>,
    last_snapshot_version: usize,
}

impl BrowserTab {
    fn normalize_url(url: &str) -> String {
        let trimmed = url.trim();
        if trimmed.starts_with("http://")
            || trimmed.starts_with("https://")
            || trimmed.starts_with("file://")
            || trimmed.starts_with("about:")
        {
            trimmed.to_string()
        } else if trimmed.starts_with("localhost") || trimmed.starts_with("127.0.0.1") {
            format!("http://{}", trimmed)
        } else {
            format!("https://{}", trimmed)
        }
    }

    pub fn new(
        initial_url: &str,
        workspace: Option<WeakEntity<Workspace>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let normalized_url = Self::normalize_url(initial_url);
        let url_input = cx.new(|cx| {
            InputField::new(window, cx, "Enter URL (e.g. http://localhost:3000)...")
                .start_icon(IconName::Public)
        });
        let editor = url_input.read(cx).editor().clone();
        editor.set_text(&normalized_url, window, cx);

        #[cfg(target_os = "macos")]
        let web_view = mac_web_view::NativeWebView::new(window, &normalized_url);

        // Periodically poll webview (every 150ms) for fast snapshot prewarming and title/URL updates
        cx.spawn(async move |this, cx| {
            let mut tick: u32 = 0;
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(150))
                    .await;
                tick = tick.wrapping_add(1);
                let result = this.update(cx, |tab, cx| {
                    #[cfg(target_os = "macos")]
                    if let Some(wv) = &tab.web_view {
                        let mut changed = false;
                        if let Some(title) = wv.get_title() {
                            if !title.is_empty() && tab.title != title {
                                tab.title = title;
                                changed = true;
                                wv.take_snapshot();
                            }
                        }
                        if let Some(url) = wv.get_url() {
                            if !url.is_empty() && tab.url != url {
                                tab.url = url.clone();
                                changed = true;
                                wv.take_snapshot();
                            }
                        }

                        // Fast prewarm: take snapshot as soon as page begins or finishes loading
                        if !wv.is_hidden() {
                            let is_loading = wv.is_loading();
                            let progress = wv.estimated_progress();
                            // Prewarm immediately on tick 3 (~450ms), or when loading completes, or when progress > 0.5
                            if wv.has_no_snapshot() && (tick >= 3 || !is_loading || progress > 0.5) {
                                wv.take_snapshot();
                            } else if tick % 10 == 0 {
                                // Keep snapshot fresh every 1.5 seconds while browsing
                                wv.take_snapshot();
                            }
                        }

                        let current_ver = wv.get_snapshot_version();
                        if current_ver != tab.last_snapshot_version {
                            tab.last_snapshot_version = current_ver;
                            changed = true;
                        }

                        if changed {
                            cx.notify();
                        }
                    }
                });
                if result.is_err() {
                    break;
                }
            }
        })
        .detach();

        Self {
            focus_handle: cx.focus_handle(),
            url: normalized_url,
            title: "Web Browser".to_string(),
            url_input,
            workspace,
            #[cfg(target_os = "macos")]
            web_view,
            last_snapshot_version: 0,
        }
    }

    pub fn navigate(&mut self, url: &str, window: &mut Window, cx: &mut Context<Self>) {
        let normalized = Self::normalize_url(url);
        self.url = normalized.clone();
        let editor = self.url_input.read(cx).editor().clone();
        editor.set_text(&self.url, window, cx);
        #[cfg(target_os = "macos")]
        if let Some(wv) = &mut self.web_view {
            wv.load_url(&normalized);
            wv.take_snapshot();
        }
        cx.notify();
    }
}

impl EventEmitter<ItemEvent> for BrowserTab {}

impl Focusable for BrowserTab {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Item for BrowserTab {
    type Event = ItemEvent;

    fn tab_content_text(&self, _detail: usize, _cx: &App) -> SharedString {
        if self.title.is_empty() {
            "Browser".into()
        } else {
            self.title.clone().into()
        }
    }

    fn tab_content(&self, params: TabContentParams, _window: &Window, _cx: &App) -> AnyElement {
        let title = if self.title.is_empty() {
            "Web Browser".to_string()
        } else {
            self.title.clone()
        };

        let words: Vec<&str> = title.split_whitespace().collect();
        if words.len() <= 3 {
            Label::new(title)
                .single_line()
                .color(params.text_color())
                .into_any_element()
        } else {
            let text_color = params.text_color();
            let char_count = title.chars().count();
            let text_width = (char_count as f32 * 7.5).max(110.0);
            let container_width = 110.0_f32;
            let scroll_distance = (text_width - container_width + 16.0).max(24.0);

            div()
                .w(px(container_width))
                .overflow_hidden()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .whitespace_nowrap()
                        .with_animation(
                            "browser-tab-marquee",
                            gpui::Animation::new(std::time::Duration::from_secs(6)).repeat(),
                            move |this, delta| {
                                let phase = if delta < 0.15 {
                                    0.0
                                } else if delta < 0.5 {
                                    (delta - 0.15) / 0.35
                                } else if delta < 0.65 {
                                    1.0
                                } else {
                                    1.0 - (delta - 0.65) / 0.35
                                };
                                let offset = phase * scroll_distance;
                                this.relative().left(px(-offset))
                            },
                        )
                        .child(Label::new(title).single_line().color(text_color)),
                )
                .into_any_element()
        }
    }

    fn tab_icon(&self, _window: &Window, _cx: &App) -> Option<Icon> {
        let url_lower = self.url.to_lowercase();
        let icon_name = if url_lower.contains("github.com") {
            IconName::Github
        } else if url_lower.contains("gitlab.com") {
            IconName::Gitlab
        } else if url_lower.contains("bitbucket.org") {
            IconName::Bitbucket
        } else if url_lower.contains("codeberg.org") {
            IconName::Codeberg
        } else if url_lower.contains("google.com") || url_lower.contains("google.") {
            IconName::AiGoogle
        } else if url_lower.contains("claude.ai") || url_lower.contains("anthropic.com") {
            IconName::AiClaude
        } else if url_lower.contains("openai.com") || url_lower.contains("chatgpt.com") {
            IconName::AiOpenAi
        } else if url_lower.contains("localhost") || url_lower.contains("127.0.0.1") {
            IconName::Server
        } else {
            IconName::Public
        };
        Some(Icon::new(icon_name))
    }

    fn deactivated(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        #[cfg(target_os = "macos")]
        if let Some(wv) = &self.web_view {
            wv.set_hidden(true);
        }
    }

    fn clone_on_split(
        &self,
        _workspace_id: Option<crate::WorkspaceId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Task<Option<Entity<Self>>> {
        let new_tab = cx.new(|cx| BrowserTab::new(&self.url, self.workspace.clone(), window, cx));
        gpui::Task::ready(Some(new_tab))
    }

    fn can_split(&self) -> bool {
        true
    }
}

impl Render for BrowserTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let this_tab = cx.entity().clone();

        let editor = self.url_input.read(cx).editor().clone();
        if editor.text(cx) != self.url && !editor.focus_handle(cx).is_focused(window) {
            editor.set_text(&self.url, window, cx);
        }

        let (modal_open, bottom_popup_open) = self
            .workspace
            .as_ref()
            .and_then(|ws| ws.upgrade())
            .map_or((false, false), |ws| {
                let ws = ws.read(cx);
                let modal = ws.modal_layer.read(cx).has_active_modal();
                let bottom_popup = !ws.notifications.is_empty() || ws.toast_layer.read(cx).has_active_toast();
                (modal, bottom_popup)
            });
        let menu_open = ui::has_active_right_click_menu() || ui::has_active_popover_menu();

        #[cfg(target_os = "macos")]
        let snapshot = self.web_view.as_ref().and_then(|wv| wv.get_snapshot());
        #[cfg(not(target_os = "macos"))]
        let snapshot: Option<Arc<gpui::Image>> = None;

        let any_overlay_open = menu_open || modal_open || bottom_popup_open;
        let should_hide = any_overlay_open && snapshot.is_some();

        #[cfg(target_os = "macos")]
        if let Some(wv) = &self.web_view {
            wv.set_hidden(should_hide);
        }

        v_flex()
            .key_context("BrowserTab")
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(cx.theme().colors().editor_background)
            // Top Navigation Toolbar
            .child(
                h_flex()
                    .h(px(38.))
                    .w_full()
                    .px_2()
                    .gap_2()
                    .items_center()
                    .border_b_1()
                    .border_color(cx.theme().colors().border)
                    .bg(cx.theme().colors().tab_bar_background)
                    // Back
                    .child(
                        IconButton::new("browser-back", IconName::ArrowLeft)
                            .tooltip(Tooltip::text("Back"))
                            .on_click(cx.listener(|this, _, _, _| {
                                #[cfg(target_os = "macos")]
                                if let Some(wv) = &this.web_view {
                                    wv.go_back();
                                }
                            })),
                    )
                    // Forward
                    .child(
                        IconButton::new("browser-forward", IconName::ArrowRight)
                            .tooltip(Tooltip::text("Forward"))
                            .on_click(cx.listener(|this, _, _, _| {
                                #[cfg(target_os = "macos")]
                                if let Some(wv) = &this.web_view {
                                    wv.go_forward();
                                }
                            })),
                    )
                    // Reload
                    .child(
                        IconButton::new("browser-reload", IconName::RotateCw)
                            .tooltip(Tooltip::text("Reload"))
                            .on_click(cx.listener(|this, _, _, _| {
                                #[cfg(target_os = "macos")]
                                if let Some(wv) = &this.web_view {
                                    wv.reload();
                                }
                            })),
                    )
                    // Address Bar
                    .child(
                        h_flex()
                            .flex_1()
                            .items_center()
                            .gap_1()
                            .on_action(cx.listener(|this, _: &Confirm, window, cx| {
                                let input_text =
                                    this.url_input.read(cx).text(cx).trim().to_string();
                                if !input_text.is_empty() {
                                    this.navigate(&input_text, window, cx);
                                }
                            }))
                            .child(self.url_input.clone())
                            .child(
                                IconButton::new("browser-go", IconName::Return)
                                    .tooltip(Tooltip::text("Go to URL"))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        let input_text =
                                            this.url_input.read(cx).text(cx).trim().to_string();
                                        if !input_text.is_empty() {
                                            this.navigate(&input_text, window, cx);
                                        }
                                    })),
                            ),
                    )
                    // External system browser button
                    .child(
                        IconButton::new("browser-external", IconName::ArrowUpRight)
                            .tooltip(Tooltip::text("Open in System Browser"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                cx.open_url(&this.url);
                            })),
                    ),
            )
            // Real Web View Viewport
            .child(
                div()
                    .flex_1()
                    .size_full()
                    .relative()
                    .bg(cx.theme().colors().editor_background)
                    // Approach 1: Pre-warmed background snapshot layer
                    // Permanently painted into Metal underneath WKWebView so transitions have zero blink
                    .children(snapshot.as_ref().map(|img_src| {
                        gpui::img(img_src.clone())
                            .absolute()
                            .inset_0()
                            .size_full()
                            .object_fit(gpui::ObjectFit::Fill)
                    }))
                    .child(
                        canvas(
                            move |bounds, window, cx| {
                                this_tab.update(cx, |this, _cx| {
                                    #[cfg(target_os = "macos")]
                                    {
                                        if bounds.size.width <= px(10.0)
                                            || bounds.size.height <= px(10.0)
                                        {
                                            if let Some(wv) = &this.web_view {
                                                wv.set_hidden(true);
                                            }
                                            return;
                                        }

                                        if this.web_view.is_none() {
                                            this.web_view =
                                                mac_web_view::NativeWebView::new(
                                                    window, &this.url,
                                                );
                                        }

                                        if let Some(wv) = &this.web_view {
                                            wv.set_hidden(should_hide);
                                            wv.update_frame(bounds);

                                            if let Some(title) = wv.get_title() {
                                                if !title.is_empty() && this.title != title
                                                {
                                                    this.title = title;
                                                }
                                            }
                                            if let Some(url) = wv.get_url() {
                                                if !url.is_empty() && this.url != url {
                                                    this.url = url;
                                                }
                                            }
                                        }
                                    }
                                });
                            },
                            |_bounds, _prepaint, _window, _cx| {},
                        )
                        .size_full(),
                    ),
            )
    }
}
