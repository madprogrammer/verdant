use crate::{render_size, renderer::Renderer, Options};
use anyhow::{bail, Context, Result};
use std::{
    os::fd::AsRawFd,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use wayland_client::{
    delegate_noop,
    protocol::{wl_callback, wl_compositor, wl_output, wl_region, wl_registry, wl_surface},
    Connection, Dispatch, Proxy, QueueHandle,
};
use wayland_protocols::wp::viewporter::client::{wp_viewport, wp_viewporter};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

static STOP: AtomicBool = AtomicBool::new(false);
static PAUSED: AtomicBool = AtomicBool::new(false);
extern "C" fn signal(number: libc::c_int) {
    if number == libc::SIGUSR1 {
        PAUSED.fetch_xor(true, Ordering::Relaxed);
    } else {
        STOP.store(true, Ordering::Relaxed);
    }
}

#[derive(Default)]
struct State {
    globals: Vec<(u32, String, u32)>,
    outputs: Vec<(u32, wl_output::WlOutput, String)>,
    selected_output: Option<u32>,
    size: Option<(u32, u32)>,
    dirty: bool,
    frame_ready: bool,
    closed: bool,
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } => state.globals.push((name, interface, version)),
            wl_registry::Event::GlobalRemove { name } => {
                if state.selected_output == Some(name) {
                    state.closed = true;
                }
                state.outputs.retain(|(id, _, _)| *id != name);
            }
            _ => {}
        }
    }
}
impl Dispatch<wl_output::WlOutput, ()> for State {
    fn event(
        state: &mut Self,
        output: &wl_output::WlOutput,
        event: wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_output::Event::Name { name } = event {
            if let Some(entry) = state.outputs.iter_mut().find(|(_, o, _)| o == output) {
                entry.2 = name;
            }
        }
    }
}
impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()> for State {
    fn event(
        state: &mut Self,
        layer: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_layer_surface_v1::Event::Configure {
                serial,
                width,
                height,
            } => {
                layer.ack_configure(serial);
                state.size = Some((width.max(1), height.max(1)));
                state.dirty = true;
            }
            zwlr_layer_surface_v1::Event::Closed => state.closed = true,
            _ => {}
        }
    }
}
impl Dispatch<wl_callback::WlCallback, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_callback::WlCallback,
        _: wl_callback::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        state.frame_ready = true;
    }
}
delegate_noop!(State: ignore wl_compositor::WlCompositor);
delegate_noop!(State: ignore wl_surface::WlSurface);
delegate_noop!(State: ignore wl_region::WlRegion);
delegate_noop!(State: ignore zwlr_layer_shell_v1::ZwlrLayerShellV1);
delegate_noop!(State: ignore wp_viewporter::WpViewporter);
delegate_noop!(State: ignore wp_viewport::WpViewport);

fn global(state: &State, interface: &str) -> Result<(u32, u32)> {
    state
        .globals
        .iter()
        .find(|(_, name, _)| name == interface)
        .map(|(id, _, version)| (*id, *version))
        .with_context(|| {
            format!("Compositor does not advertise {interface}; Niri, Sway or Hyprland is required")
        })
}

pub fn run(options: &Options) -> Result<()> {
    let connection = Connection::connect_to_env().context("Connecting to Wayland")?;
    let mut queue = connection.new_event_queue();
    let qh = queue.handle();
    let registry = connection.display().get_registry(&qh, ());
    let mut state = State {
        frame_ready: true,
        ..Default::default()
    };
    queue.roundtrip(&mut state)?;
    for (id, interface, version) in &state.globals {
        if interface == "wl_output" {
            state.outputs.push((
                *id,
                registry.bind::<wl_output::WlOutput, _, _>(*id, (*version).min(4), &qh, ()),
                format!("output-{id}"),
            ));
        }
    }
    queue.roundtrip(&mut state)?;
    if options.list_outputs {
        for (_, _, name) in &state.outputs {
            println!("{name}");
        }
        return Ok(());
    }
    let output = if let Some(name) = &options.output {
        let (id, output, _) = state
            .outputs
            .iter()
            .find(|(_, _, n)| n == name)
            .with_context(|| format!("Unknown output {name}; use --list-outputs"))?;
        state.selected_output = Some(*id);
        Some(output.clone())
    } else {
        None
    };
    let (id, version) = global(&state, "wl_compositor")?;
    let compositor: wl_compositor::WlCompositor = registry.bind(id, version.min(4), &qh, ());
    let (id, _) = global(&state, "zwlr_layer_shell_v1")?;
    let shell: zwlr_layer_shell_v1::ZwlrLayerShellV1 = registry.bind(id, 1, &qh, ());
    let (id, _) = global(&state, "wp_viewporter")?;
    let viewporter: wp_viewporter::WpViewporter = registry.bind(id, 1, &qh, ());
    let surface = compositor.create_surface(&qh, ());
    let empty = compositor.create_region(&qh, ());
    surface.set_input_region(Some(&empty));
    empty.destroy();
    let layer = shell.get_layer_surface(
        &surface,
        output.as_ref(),
        zwlr_layer_shell_v1::Layer::Background,
        "verdant".into(),
        &qh,
        (),
    );
    layer.set_anchor(
        zwlr_layer_surface_v1::Anchor::Top
            | zwlr_layer_surface_v1::Anchor::Bottom
            | zwlr_layer_surface_v1::Anchor::Left
            | zwlr_layer_surface_v1::Anchor::Right,
    );
    layer.set_keyboard_interactivity(zwlr_layer_surface_v1::KeyboardInteractivity::None);
    layer.set_exclusive_zone(-1);
    layer.set_size(0, 0);
    let viewport = viewporter.get_viewport(&surface, &qh, ());
    surface.commit();
    while state.size.is_none() && !state.closed {
        queue.blocking_dispatch(&mut state)?;
    }
    if state.closed {
        bail!("Compositor closed the background layer before configuration");
    }
    let (width, height) = state.size.unwrap();
    let (rw, rh) = render_size(width, height, options.max_width);
    let window = wayland_egl::WlEglSurface::new(surface.id(), rw as i32, rh as i32)?;
    // Renderer drops before the native window, and both outlive all drawing.
    let mut renderer = unsafe {
        Renderer::new(
            connection.backend().display_ptr().cast(),
            Some(window.ptr().cast_mut()),
            rw,
            rh,
            options,
        )?
    };
    unsafe {
        libc::signal(libc::SIGINT, signal as *const () as usize);
        libc::signal(libc::SIGTERM, signal as *const () as usize);
        libc::signal(libc::SIGUSR1, signal as *const () as usize);
    }
    eprintln!(
        "Verdant · {rw}×{rh} · {} columns · {} fps cap",
        options.columns, options.fps
    );
    eprintln!("SIGUSR1 toggles pause; Ctrl-C or SIGTERM exits.");
    let start = Instant::now();
    let interval = Duration::from_secs_f64(1.0 / options.fps as f64);
    let mut next_frame = Instant::now();
    let mut animation_time = options.time;
    let mut last_tick = Instant::now();
    let mut frames = 0u64;
    loop {
        queue.dispatch_pending(&mut state)?;
        let now = Instant::now();
        if state.closed
            || STOP.load(Ordering::Relaxed)
            || options
                .duration
                .is_some_and(|d| start.elapsed().as_secs_f64() >= d)
        {
            break;
        }
        let paused = PAUSED.load(Ordering::Relaxed) || options.still || options.speed == 0.0;
        if !paused {
            animation_time += now.duration_since(last_tick).as_secs_f32() * options.speed;
        }
        last_tick = now;
        if state.dirty {
            let (w, h) = state.size.unwrap();
            let (rw, rh) = render_size(w, h, options.max_width);
            window.resize(rw as i32, rh as i32, 0, 0);
            renderer.width = rw;
            renderer.height = rh;
            viewport.set_destination(w as i32, h as i32);
            let opaque = compositor.create_region(&qh, ());
            opaque.add(0, 0, w as i32, h as i32);
            surface.set_opaque_region(Some(&opaque));
            opaque.destroy();
        }
        if state.dirty || (!paused && state.frame_ready && now >= next_frame) {
            renderer.draw(animation_time, options);
            // At most one pending frame callback; callbacks also throttle hidden outputs.
            if state.frame_ready {
                surface.frame(&qh, ());
                state.frame_ready = false;
            }
            renderer.swap()?;
            state.dirty = false;
            frames += 1;
            next_frame = Instant::now() + interval;
        }
        connection.flush()?;
        let mut timeout = Duration::from_secs(1);
        if !paused && state.frame_ready {
            timeout = next_frame.saturating_duration_since(Instant::now());
        }
        if let Some(duration) = options.duration {
            timeout =
                timeout.min(Duration::from_secs_f64(duration).saturating_sub(start.elapsed()));
        }
        if let Some(guard) = connection.prepare_read() {
            let mut fd = libc::pollfd {
                fd: guard.connection_fd().as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            let millis = timeout.as_millis().saturating_add(1).min(i32::MAX as u128) as i32;
            let result = unsafe { libc::poll(&mut fd, 1, millis) };
            if result > 0 {
                guard.read()?;
            } else if result < 0
                && std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted
            {
                return Err(std::io::Error::last_os_error().into());
            }
        }
    }
    eprintln!(
        "Rendered {frames} frames in {:.1}s",
        start.elapsed().as_secs_f64()
    );
    drop(renderer);
    drop(window);
    viewport.destroy();
    layer.destroy();
    surface.destroy();
    connection.flush()?;
    Ok(())
}
