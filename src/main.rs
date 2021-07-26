use log::{debug, error, info};
use pixels::{Pixels, PixelsBuilder, SurfaceTexture};
use rand::Rng;
use std::{
    cell::Cell,
    collections::{HashSet, VecDeque},
    time::{Duration, Instant},
};
use winit::{
    dpi::PhysicalSize,
    event::{ElementState, Event, KeyboardInput, StartCause, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowBuilder},
};

#[derive(Clone, Copy)]
struct Color(u32);

impl Color {
    const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Color(r as u32 | ((g as u32) << 8) | ((b as u32) << 16) | 0xFF000000)
    }

    const fn as_rgba_u32(self) -> u32 {
        self.0
    }
}

fn clamp<T: PartialOrd>(input: T, min: T, max: T) -> T {
    if input < min {
        min
    } else if input > max {
        max
    } else {
        input
    }
}

fn pixels_slice_u32_mut(pixels: &mut Pixels) -> &mut [u32] {
    unsafe {
        let (_, pixel_array, _) = pixels.get_frame().align_to_mut::<u32>();
        pixel_array
    }
}

struct Canvas {
    width: usize,
    height: usize,
    pixels: Pixels,
    frame_times: VecDeque<Instant>,
}

impl Canvas {
    fn new(window: &Window, width: u32, height: u32) -> Result<Self, pixels::Error> {
        let window_size = window.inner_size();
        let pixels = PixelsBuilder::new(
            width,
            height,
            SurfaceTexture::new(window_size.width, window_size.height, window),
        )
        .enable_vsync(true)
        .present_mode(wgpu::PresentMode::Immediate)
        .build()?;

        Ok(Canvas {
            width: width as usize,
            height: height as usize,
            pixels,
            frame_times: VecDeque::new(),
        })
    }

    fn update_fps(&mut self) {
        let now = Instant::now();
        self.frame_times.push_back(now);
        let second_ago = now - Duration::from_secs(1);
        while *self.frame_times.front().unwrap() < second_ago {
            self.frame_times.pop_front().unwrap();
        }
    }

    fn fps(&self) -> f32 {
        self.frame_times.len() as f32
    }

    fn draw(&mut self) -> Result<(), ()> {
        self.update_fps();
        self.pixels.render().map_err(|e| {
            error!("Pixels error: {}", e);
        })
    }

    fn clear(&mut self, color: Color) {
        pixels_slice_u32_mut(&mut self.pixels).fill(color.as_rgba_u32())
    }

    fn set_pixel(&mut self, x: i32, y: i32, color: Color) {
        if x < 0 || y < 0 {
            return;
        }
        let (x, y) = (x as usize, y as usize);
        if x >= self.width || y >= self.height {
            return;
        }

        pixels_slice_u32_mut(&mut self.pixels)[self.width * (self.height - y - 1) + x] =
            color.as_rgba_u32()
    }

    fn fill_rectangle(&mut self, x0: i32, y0: i32, w: usize, h: usize, color: Color) {
        let x0 = clamp(x0, 0, self.width as i32) as usize;
        let y0 = clamp(y0, 0, self.height as i32) as usize;
        let h = if h > y0 { y0 } else { h };

        let mut offset = self.width * (self.height - y0 - 1) + x0;

        let slice = pixels_slice_u32_mut(&mut self.pixels);
        for _ in 0..h {
            slice[offset..offset + w].fill(color.as_rgba_u32());
            offset += self.width;
        }
    }

    fn resize_surface(&mut self, width: u32, height: u32) {
        self.pixels.resize_surface(width, height);
    }
}

#[derive(Clone, Copy, Debug, Default, Hash)]
struct Vec2(i32, i32);

impl std::ops::AddAssign for Vec2 {
    fn add_assign(&mut self, other: Self) {
        self.0 += other.0;
        self.1 += other.1;
    }
}

impl std::ops::Add for Vec2 {
    type Output = Vec2;

    fn add(self, other: Self) -> Self {
        Vec2(self.0 + other.0, self.1 + other.1)
    }
}

impl std::cmp::PartialEq for Vec2 {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0 && self.1 == other.1
    }
}

impl std::cmp::Eq for Vec2 {}

const BG_COLOR: Color = Color::rgb(0x48, 0xB2, 0xE8);
const HEAD_COLOR: Color = Color::rgb(0x4E, 0x38, 0xE8);
const TAIL_COLOR: Color = Color::rgb(0x5E, 0x48, 0xE8);
const FOOD_COLOR: Color = Color::rgb(0x9E, 0x28, 0xE8);

struct State {
    tick: Duration,
    food_tick: Duration,
    next_update: Instant,
    next_food: Instant,
    fps_update: Cell<Instant>,

    width: i32,
    height: i32,
    v: Vec2,
    head: Vec2,
    tail: Vec<Vec2>,
    food: HashSet<Vec2>,
}

impl State {
    fn new() -> Self {
        let tick = Duration::from_millis(400);
        let food_tick = Duration::from_millis(1500);
        State {
            tick,
            next_update: Instant::now() + tick,
            food_tick,
            next_food: Instant::now() + food_tick,
            fps_update: Cell::new(Instant::now()),
            width: 15,
            height: 15,
            v: Vec2(1, 0),
            head: Vec2(8, 7),
            tail: vec![Vec2(7, 7), Vec2(6, 7)],
            food: HashSet::new(),
        }
    }

    fn update(&mut self) -> bool {
        if Instant::now() > self.next_update {
            if self.step() {
                return true;
            }
            self.next_update = Instant::now() + self.tick;
        }

        if self.food.is_empty() || Instant::now() > self.next_food {
            if self.add_food() {
                return true;
            }
            self.next_food = Instant::now() + self.food_tick;
        }

        false
    }

    fn step(&mut self) -> bool {
        let new_head = self.head + self.v;

        if new_head.0 < 0 || new_head.0 >= self.width ||
           new_head.1 < 0 || new_head.1 >= self.height ||
           self.tail[0..self.tail.len() - 1].contains(&new_head) {
            return true;
        }

        if self.food.contains(&new_head) {
            self.tail.push(Vec2(0, 0));
            self.food.remove(&new_head);
        }

        for i in (0..(self.tail.len() - 1)).rev() {
            self.tail[i + 1] = self.tail[i];
        }
        self.tail[0] = self.head;
        self.head += self.v;
        false
    }

    fn add_food(&mut self) -> bool {
        let total_nodes = self.width * self.height;
        if self.tail.len() + self.food.len() + 2 >= total_nodes as usize {
            return true;
        }

        loop {
            let idx = rand::thread_rng().gen_range(0..total_nodes);
            let pos = Vec2(idx % self.width, idx / self.width);
            if !self.food.contains(&pos) && !self.tail.contains(&pos) && pos != self.head {
                self.food.insert(pos);
                return false;
            }
        }
    }

    fn render(&self, canvas: &mut Canvas) {
        canvas.clear(BG_COLOR);
        canvas.set_pixel(self.head.0, self.head.1, HEAD_COLOR);
        for pos in self.tail.iter() {
            canvas.set_pixel(pos.0, pos.1, TAIL_COLOR);
        }
        for pos in self.food.iter() {
            canvas.set_pixel(pos.0, pos.1, FOOD_COLOR);
        }
        if Instant::now() > self.fps_update.get() {
            info!("FPS: {}", canvas.fps());
            self.fps_update.set(Instant::now() + Duration::from_secs(1))
        }
    }

    fn on_keypress(&mut self, keycode: VirtualKeyCode) {
        match keycode {
            VirtualKeyCode::Right => {
                self.v = Vec2(1, 0);
            }
            VirtualKeyCode::Up => {
                self.v = Vec2(0, 1);
            }
            VirtualKeyCode::Left => {
                self.v = Vec2(-1, 0);
            }
            VirtualKeyCode::Down => {
                self.v = Vec2(0, -1);
            }
            _ => (),
        }
    }
}

fn handle_keypress(keycode: VirtualKeyCode, state: &mut State) -> Option<ControlFlow> {
    match keycode {
        VirtualKeyCode::Escape => Some(ControlFlow::Exit),
        x => {
            state.on_keypress(x);
            None
        }
    }
}

fn handle_event<T: std::fmt::Debug + 'static>(
    event: Event<T>,
    state: &mut State,
    canvas: &mut Canvas,
) -> Option<ControlFlow> {
    match &event {
        Event::NewEvents(StartCause::Init) => {
            info!("Initializing events");
            Some(ControlFlow::Poll)
        }
        Event::NewEvents(StartCause::Poll) | Event::NewEvents(StartCause::WaitCancelled { .. }) => {
            if state.update() {
                return Some(ControlFlow::Exit)
            }
            state.render(canvas);
            if canvas.draw().is_err() {
                Some(ControlFlow::Exit)
            } else {
                None
            }
        }
        Event::NewEvents(_) => {
            debug!("Event: {:?}", event);
            None
        }
        Event::WindowEvent {
            event: window_event,
            ..
        } => {
            debug!("WindowEvent:  {:?}", window_event);
            match window_event {
                WindowEvent::Resized(PhysicalSize { width, height }) => {
                    info!("Window resized to ({}, {})", width, height);
                    canvas.resize_surface(*width, *height);
                    None
                }
                WindowEvent::CloseRequested => Some(ControlFlow::Exit),
                WindowEvent::KeyboardInput {
                    input:
                        KeyboardInput {
                            virtual_keycode: Some(keycode),
                            state: ElementState::Pressed,
                            ..
                        },
                    ..
                } => handle_keypress(*keycode, state),
                WindowEvent::KeyboardInput { .. } => None,
                _ => None,
            }
        }
        Event::RedrawRequested(_) => {
            debug!("RedrawRequested");
            state.render(canvas);
            if canvas.draw().is_err() {
                Some(ControlFlow::Exit)
            } else {
                None
            }
        }
        Event::DeviceEvent { .. } => None,
        Event::MainEventsCleared => None,
        Event::RedrawEventsCleared => None,
        _ => {
            debug!("Event:  {:?}", event);
            None
        }
    }
}

fn main() {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("snake_pixels=debug"),
    )
    .format_timestamp(Some(env_logger::fmt::TimestampPrecision::Micros))
    .init();
    info!("Starting up");

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new().build(&event_loop).unwrap();
    info!("Created window");

    let mut canvas = Canvas::new(&window, 15, 15).unwrap();
    info!("Initialized canvas");

    let mut state = State::new();

    event_loop.run(move |event, _, control_flow| {
        handle_event(event, &mut state, &mut canvas).map(|cf| {
            debug!("Setting ControlFlow {:?}", cf);
            *control_flow = cf
        });
    });
}
