use log::warn;
use raw_window_handle::{
    HasRawDisplayHandle, HasRawWindowHandle, RawDisplayHandle, RawWindowHandle, WebDisplayHandle,
    WebWindowHandle,
};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::RwLock;
use std::time::Duration;
use wasm_bindgen::prelude::*;
use wgpu::util::DeviceExt;

struct WebWindow;
unsafe impl HasRawDisplayHandle for WebWindow {
    fn raw_display_handle(&self) -> RawDisplayHandle {
        RawDisplayHandle::Web(WebDisplayHandle::empty())
    }
}
unsafe impl HasRawWindowHandle for WebWindow {
    fn raw_window_handle(&self) -> RawWindowHandle {
        RawWindowHandle::Web(WebWindowHandle::empty())
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    mouse_pos: [f32; 2],
    seed: [f32; 2],
}

impl Uniforms {
    fn new() -> Self {
        Self {
            mouse_pos: [-1000.0, 0.0],
            seed: [0.0, 0.0],
        }
    }
}

struct State {
    surface: wgpu::Surface,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    compute_pipeline: wgpu::RenderPipeline,
    render_pipeline: wgpu::RenderPipeline,
    mousedown: RwLock<bool>,
    last_mousepos: RwLock<Option<(u32, u32)>>,
    start_mousepos: RwLock<Option<(u32, u32)>>,
    texture_size: wgpu::Extent3d,
    texture: wgpu::Texture,
    texture_target: wgpu::Texture,
    texture_target_view: wgpu::TextureView,
    texture_bind_group: wgpu::BindGroup,
    texture_target_bind_group: wgpu::BindGroup,
    uniforms: RwLock<Uniforms>,
    uniforms_buffer: wgpu::Buffer,
    uniforms_bind_group: wgpu::BindGroup,
}

#[derive(Debug)]
enum CanvasEvent {
    MouseMove(u32, u32),
    MouseDown,
    MouseUp,
}

impl State {
    async fn new(canvas: &web_sys::HtmlCanvasElement) -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            dx12_shader_compiler: Default::default(),
        });

        let surface = unsafe { instance.create_surface_from_canvas(&canvas) }.unwrap();

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    features: wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES,
                    limits: wgpu::Limits::downlevel_webgl2_defaults()
                        .using_resolution(adapter.limits()),
                    label: None,
                },
                None,
            )
            .await
            .unwrap();

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .filter(|f| f.describe().srgb)
            .next()
            .unwrap_or(surface_caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            width: 1024,
            height: 1024,
        };

        surface.configure(&device, &config);

        let texture_size = wgpu::Extent3d {
            width: 1024,
            height: 1024,
            depth_or_array_layers: 1,
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            view_formats: &[wgpu::TextureFormat::Rgba32Float],
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_DST,
        });

        let texture_target = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            view_formats: &[wgpu::TextureFormat::Rgba32Float],
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC,
        });

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        multisampled: false,
                    },
                    count: None,
                }],
            });

        let texture_target_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        multisampled: false,
                    },
                    count: None,
                }],
            });

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let texture_target_view =
            texture_target.create_view(&wgpu::TextureViewDescriptor::default());

        let texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &texture_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&texture_view),
            }],
        });

        let texture_target_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &texture_target_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&texture_target_view),
            }],
        });

        let uniforms = Uniforms::new();
        let uniforms_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&[uniforms]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let uniforms_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
                label: None,
            });
        let uniforms_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &uniforms_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms_buffer.as_entire_binding(),
            }],
            label: None,
        });

        // Create pipeline
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let compute_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[&texture_bind_group_layout, &uniforms_bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[
                    &texture_target_bind_group_layout,
                    &uniforms_bind_group_layout,
                ],
                push_constant_ranges: &[],
            });

        let compute_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Compute Pipeline"),
            layout: Some(&compute_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_compute",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_compute",
                targets: &[Some(wgpu::TextureFormat::Rgba32Float.into())],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

        Self {
            surface,
            device,
            queue,
            config,
            compute_pipeline,
            render_pipeline,
            mousedown: RwLock::new(false),
            last_mousepos: RwLock::new(None),
            start_mousepos: RwLock::new(None),
            texture_size,
            texture,
            texture_target,
            texture_target_view,
            texture_bind_group,
            texture_target_bind_group,
            uniforms: RwLock::new(uniforms),
            uniforms_buffer,
            uniforms_bind_group,
        }
    }

    fn input(&self, event: &CanvasEvent) -> bool {
        warn!("{:?}", &event);
        match event {
            CanvasEvent::MouseDown => {
                *self.mousedown.write().unwrap() = true;
                *self.start_mousepos.write().unwrap() = *self.last_mousepos.read().unwrap();
            }
            CanvasEvent::MouseUp => {
                *self.mousedown.write().unwrap() = false;
            }
            CanvasEvent::MouseMove(x, y) => {
                let old_mousepos = *self.last_mousepos.read().unwrap();
                *self.last_mousepos.write().unwrap() = Some((*x, *y));
                if !*self.mousedown.read().unwrap() || old_mousepos.is_none() {
                    return false;
                }
            }
            _ => {}
        }
        false
    }

    fn update(&self) {
        let MOUSE_INACTIVE = [-1000.0, 0.0];
        let mut mousepos = self
            .last_mousepos
            .read()
            .unwrap()
            .map_or(MOUSE_INACTIVE, |(x, y)| [x as f32, y as f32]);
        let mut seed = self
            .start_mousepos
            .read()
            .unwrap()
            .map_or(MOUSE_INACTIVE, |(x, y)| [x as f32, y as f32]);

        if !*self.mousedown.read().unwrap() {
            mousepos = MOUSE_INACTIVE;
        }

        warn!("{:?}", &mousepos);
        self.uniforms.write().unwrap().mouse_pos = mousepos;
        self.uniforms.write().unwrap().seed = seed;
        self.queue.write_buffer(
            &self.uniforms_buffer,
            0,
            bytemuck::cast_slice(&[*self.uniforms.read().unwrap()]),
        );
    }

    fn render(&self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            {
                let mut compute_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("compute pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.texture_target_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.1,
                                g: 0.2,
                                b: 0.3,
                                a: 1.0,
                            }),
                            store: true,
                        },
                    })],
                    depth_stencil_attachment: None,
                });

                compute_pass.set_pipeline(&self.compute_pipeline);
                compute_pass.set_bind_group(0, &self.texture_bind_group, &[]);
                compute_pass.set_bind_group(1, &self.uniforms_bind_group, &[]);
                compute_pass.draw(0..3, 0..1);
            }

            {
                encoder.copy_texture_to_texture(
                    wgpu::ImageCopyTextureBase {
                        texture: &self.texture_target,
                        mip_level: 0,
                        origin: wgpu::Origin3d::default(),
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::ImageCopyTextureBase {
                        texture: &self.texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::default(),
                        aspect: wgpu::TextureAspect::All,
                    },
                    self.texture_size,
                );
            }

            {
                let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("render pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.1,
                                g: 0.2,
                                b: 0.3,
                                a: 1.0,
                            }),
                            store: true,
                        },
                    })],
                    depth_stencil_attachment: None,
                });

                render_pass.set_pipeline(&self.render_pipeline);
                render_pass.set_bind_group(0, &self.texture_target_bind_group, &[]);
                render_pass.set_bind_group(1, &self.uniforms_bind_group, &[]);
                render_pass.draw(0..3, 0..1);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen(start))]
pub async fn run() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    console_log::init_with_level(log::Level::Warn).expect("Couldn't initialize logger");

    let window = web_sys::window().unwrap();
    let doc = window.document().unwrap();
    let canvas = doc.get_element_by_id("canvas").unwrap();

    let canvas: &'static _ = Box::leak(Box::new(
        canvas.dyn_into::<web_sys::HtmlCanvasElement>().unwrap(),
    ));

    canvas.set_width(1024);
    canvas.set_height(1024);

    let state = Arc::new(State::new(&canvas).await);

    let mut receiver = setup_listeners(&canvas);

    {
        let state2 = Arc::clone(&state);
        let window2 = window.clone();

        let f = Rc::new(RefCell::<Option<Closure<dyn FnMut()>>>::new(None));
        let g = f.clone();
        *g.borrow_mut() = Some(Closure::new(move || {
            state2.render().unwrap();
            window2.request_animation_frame(f.borrow().as_ref().unwrap().as_ref().unchecked_ref());
        }));

        window.request_animation_frame(g.borrow().as_ref().unwrap().as_ref().unchecked_ref());
    }

    loop {
        tokio::select! {
            Some(event) = receiver.recv() => {
                state.input(&event);
                state.update();
            }
        }
    }
}

fn setup_listeners(
    canvas: &'static web_sys::HtmlCanvasElement,
) -> tokio::sync::mpsc::UnboundedReceiver<CanvasEvent> {
    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();

    let sender2 = sender.clone();
    {
        let closure = Closure::wrap(Box::new(move |event: web_sys::MouseEvent| {
            let rect = canvas.get_bounding_client_rect();
            let width = canvas.width() as f32;
            let height = canvas.height() as f32;
            let x = event.offset_x() as f32 * (width / rect.width() as f32);
            let y = event.offset_y() as f32 * (height / rect.height() as f32);
            sender2.send(CanvasEvent::MouseMove(x as u32, y as u32));
        }) as Box<dyn FnMut(_)>);

        canvas
            .add_event_listener_with_callback("mousemove", closure.as_ref().unchecked_ref())
            .unwrap();
        closure.forget();
    }

    let sender2 = sender.clone();
    {
        let closure = Closure::wrap(Box::new(move |event: web_sys::MouseEvent| {
            sender2.send(CanvasEvent::MouseDown);
        }) as Box<dyn FnMut(_)>);

        canvas
            .add_event_listener_with_callback("mousedown", closure.as_ref().unchecked_ref())
            .unwrap();
        closure.forget();
    }

    let sender2 = sender.clone();
    {
        let closure = Closure::wrap(Box::new(move |event: web_sys::MouseEvent| {
            sender2.send(CanvasEvent::MouseUp);
        }) as Box<dyn FnMut(_)>);

        canvas
            .add_event_listener_with_callback("mouseup", closure.as_ref().unchecked_ref())
            .unwrap();
        closure.forget();
    }

    receiver
}
