use gtk::{
    gdk,
    glib::{self, prelude::*},
};
use std::{cell::Cell, ffi::c_int, mem::MaybeUninit, ptr, rc::Rc};
use x11_dl::xlib::{self, Display, Window, XRectangle, XWindowAttributes, Xlib};

const SHAPE_BOUNDING: c_int = 0;
const SHAPE_INPUT: c_int = 2;
const SHAPE_SET: c_int = 0;

#[link(name = "Xext")]
unsafe extern "C" {
    fn XShapeGetRectangles(
        display: *mut Display,
        window: Window,
        kind: c_int,
        count: *mut c_int,
        ordering: *mut c_int,
    ) -> *mut XRectangle;
    fn XShapeCombineRectangles(
        display: *mut Display,
        window: Window,
        kind: c_int,
        x: c_int,
        y: c_int,
        rectangles: *mut XRectangle,
        count: c_int,
        operation: c_int,
        ordering: c_int,
    );
}

unsafe extern "C" {
    fn gdk_x11_display_get_xdisplay(display: *mut gdk::ffi::GdkDisplay) -> *mut Display;
    fn gdk_x11_display_error_trap_push(display: *mut gdk::ffi::GdkDisplay);
    fn gdk_x11_display_error_trap_pop(display: *mut gdk::ffi::GdkDisplay) -> c_int;
}

pub(super) fn follow_icon(icon: &glib::Object) {
    if !std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|desktop| {
        desktop
            .split(':')
            .any(|name| name.eq_ignore_ascii_case("GNOME"))
    }) {
        return;
    }
    let applied = Rc::new(Cell::new(None));
    schedule_clip(icon, &applied);
    let on_embed = Rc::clone(&applied);
    icon.connect_notify_local(Some("embedded"), move |icon, _| {
        schedule_clip(icon, &on_embed);
    });
    icon.connect_local("size-changed", false, move |args| {
        if let Ok(icon) = args[0].get::<glib::Object>() {
            schedule_clip(&icon, &applied);
        }
        Some(false.to_value()) // Allow GTK to resize the image as usual.
    });
}

fn schedule_clip(icon: &glib::Object, applied: &Rc<Cell<Option<(Window, i32)>>>) {
    let icon = icon.downgrade();
    let applied = Rc::clone(applied);
    // Wait for GTK to apply the embedded window's allocation.
    glib::idle_add_local_once(move || {
        if let Some(icon) = icon.upgrade() {
            clip(&icon, &applied);
        }
    });
}

fn clip(icon: &glib::Object, applied: &Cell<Option<(Window, i32)>>) {
    if !icon.property::<bool>("embedded") {
        applied.set(None);
        return;
    }
    let Some((root, _)) = super::icon_geometry(icon) else {
        return;
    };
    let display = root.display();
    if !display.supports_input_shapes() {
        return;
    }
    let Ok(xlib) = Xlib::open() else {
        return;
    };
    // This path only receives GtkStatusIcon objects on GDK's X11 backend.
    // The tray host can destroy the socket during these X11 requests.
    unsafe {
        let gdk_display = display.as_ptr();
        let display = gdk_x11_display_get_xdisplay(gdk_display);
        let plug = gtk::ffi::gtk_status_icon_get_x11_window_id(icon.as_ptr().cast()).into();
        gdk_x11_display_error_trap_push(gdk_display);
        let next = clip_socket(&xlib, display, plug, applied.get());
        if gdk_x11_display_error_trap_pop(gdk_display) == 0
            && let Some(next) = next
        {
            applied.set(Some(next));
        }
    }
}

fn clip_socket(
    xlib: &Xlib,
    display: *mut Display,
    plug: Window,
    applied: Option<(Window, i32)>,
) -> Option<(Window, i32)> {
    let (root, socket, _) = window_tree(xlib, display, plug)?;
    if socket == root {
        return None;
    }
    let (_, parent, child) = window_tree(xlib, display, socket)?;
    let outer = window_attributes(xlib, display, socket)?;
    let inner = window_attributes(xlib, display, plug)?;
    let side = outer.width;
    if parent != root
        || child != Some(plug)
        || outer.override_redirect == 0
        || outer.depth != 24
        || outer.border_width != 0
        || !(8..=128).contains(&side)
        || outer.height != side
        || inner.width != side
        || inner.height != side
        || inner.x != 0
        || inner.y != 0
        || applied == Some((socket, side))
    {
        return None;
    }

    // GNOME clones a dedicated XEmbed socket into its panel and forwards clicks
    // itself. Only clip a socket that already has an empty X11 input shape.
    unsafe {
        let mut count = -1;
        let mut ordering = 0;
        let input = XShapeGetRectangles(display, socket, SHAPE_INPUT, &mut count, &mut ordering);
        if !input.is_null() {
            (xlib.XFree)(input.cast());
        }
        if count != 0 {
            return None;
        }
        let mut rows = icon_rows(side as u32);
        XShapeCombineRectangles(
            display,
            socket,
            SHAPE_BOUNDING,
            0,
            0,
            rows.as_mut_ptr(),
            rows.len() as c_int,
            SHAPE_SET,
            xlib::YSorted,
        );
    }
    Some((socket, side))
}

fn icon_rows(side: u32) -> Vec<XRectangle> {
    let pixels = crate::icons::rgba(side);
    pixels
        .chunks_exact(side as usize * 4)
        .enumerate()
        .filter_map(|(y, row)| {
            let (pixels, _) = row.as_chunks::<4>();
            let visible = |pixel: &[u8; 4]| pixel[3] >= 128;
            let left = pixels.iter().position(visible)?;
            let right = pixels.iter().rposition(visible)?;
            Some(XRectangle {
                x: left as i16,
                y: y as i16,
                width: (right - left + 1) as u16,
                height: 1,
            })
        })
        .collect()
}

fn window_tree(
    xlib: &Xlib,
    display: *mut Display,
    window: Window,
) -> Option<(Window, Window, Option<Window>)> {
    let (mut root, mut parent, mut count) = (0, 0, 0);
    let mut children: *mut Window = ptr::null_mut();
    // XQueryTree allocates the child list using Xlib's allocator.
    unsafe {
        if (xlib.XQueryTree)(
            display,
            window,
            &mut root,
            &mut parent,
            &mut children,
            &mut count,
        ) == 0
        {
            return None;
        }
        let child = (count == 1 && !children.is_null()).then(|| *children);
        if !children.is_null() {
            (xlib.XFree)(children.cast());
        }
        Some((root, parent, child))
    }
}

fn window_attributes(
    xlib: &Xlib,
    display: *mut Display,
    window: Window,
) -> Option<XWindowAttributes> {
    let mut attributes = MaybeUninit::uninit();
    // XGetWindowAttributes initializes the output only on success.
    unsafe {
        ((xlib.XGetWindowAttributes)(display, window, attributes.as_mut_ptr()) != 0)
            .then(|| attributes.assume_init())
    }
}
