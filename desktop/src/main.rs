use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use iced::futures::SinkExt;
use iced::wgpu::util::DeviceExt;
use iced::widget::{Column, Stack, center, shader, text};
use iced::{Alignment, Element, Length, Subscription, Theme, wgpu};

use kinshare_shared::messages::Info;
use tokio::sync::mpsc;

pub fn main() -> iced::Result {
    iced::application(State::default, State::update, State::view)
        .subscription(State::subscription)
        .title(State::title)
        .theme(State::theme)
        .run()
}

struct State {
    messages: Vec<&'static str>,
    stream: Option<Arc<StreamState>>,
}

#[derive(Debug)]
struct StreamState {
    info: Info,
    updated: AtomicBool,
    framebuffer: Arc<Mutex<Box<[u8]>>>,
}

#[derive(Debug, Clone)]
enum Message {
    Server(kinshare_server::Message),
}

impl State {
    fn default() -> Self {
        Self {
            messages: vec!["Initializing..."],
            stream: None,
        }
    }

    fn update(&mut self, Message::Server(server): Message) {
        match server {
            kinshare_server::Message::Message(message) => self.messages.push(message),
            kinshare_server::Message::Connected { info, framebuffer } => {
                self.stream = Some(Arc::new(StreamState {
                    info,
                    updated: AtomicBool::new(true),
                    framebuffer,
                }))
            }
            kinshare_server::Message::Updated => {
                let Some(stream) = &self.stream else {
                    return;
                };

                stream.updated.store(true, Ordering::Relaxed);
            }
            kinshare_server::Message::Closed => self.stream = None,
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let mut stack = Stack::new().width(Length::Fill).height(Length::Fill);

        if let Some(stream) = &self.stream {
            stack = stack.push(
                shader(KindleView { stream })
                    .width(Length::Fill)
                    .height(Length::Fill),
            );
        } else {
            stack = stack.push(center(
                Column::with_children(self.messages.iter().map(|msg| text!("{}", msg).into()))
                    .align_x(Alignment::Center)
                    .spacing(4.0),
            ))
        }

        stack.into()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::run(stream)
    }

    fn title(&self) -> String {
        "Kindle".to_owned()
    }

    fn theme(&self) -> Option<Theme> {
        Some(Theme::Light)
    }
}

fn stream() -> impl iced::futures::Stream<Item = Message> {
    iced::stream::channel(64, async |mut output| {
        let (sender, mut receiver) = mpsc::unbounded_channel();

        tokio::spawn(async { kinshare_server::run(sender).await });

        while let Some(message) = receiver.recv().await {
            output.send(Message::Server(message)).await.ok();
        }
    })
}

struct KindleView<'a> {
    stream: &'a Arc<StreamState>,
}

impl<Message> shader::Program<Message> for KindleView<'_> {
    type State = ();
    type Primitive = KindlePrimitive;

    fn draw(
        &self,
        _state: &Self::State,
        _cursor: iced::mouse::Cursor,
        _bounds: iced::Rectangle,
    ) -> Self::Primitive {
        KindlePrimitive {
            stream: Arc::clone(self.stream),
        }
    }
}

#[derive(Debug)]
struct KindlePrimitive {
    stream: Arc<StreamState>,
}

impl shader::Primitive for KindlePrimitive {
    type Pipeline = KindlePipeline;

    fn prepare(
        &self,
        pipeline: &mut Self::Pipeline,
        device: &iced::wgpu::Device,
        queue: &iced::wgpu::Queue,
        _bounds: &iced::Rectangle,
        viewport: &iced::widget::shader::Viewport,
    ) {
        let standard_uniform = StandardUniform {
            screen_size: [
                viewport.physical_width() as f32,
                viewport.physical_height() as f32,
            ],
            kindle_size: [
                self.stream.info.display_width as f32,
                self.stream.info.display_height as f32,
            ],
        };

        if pipeline.inner.is_none() {
            pipeline.init(
                device,
                self.stream.info.display_width as u32,
                self.stream.info.display_height as u32,
                standard_uniform,
            );
        }

        if self.stream.updated.swap(false, Ordering::AcqRel) {
            pipeline.update_screen(
                queue,
                self.stream.info.display_width as u32,
                self.stream.info.display_height as u32,
                &self.stream.framebuffer.lock().unwrap(),
            );
        }

        pipeline.update_standard(queue, standard_uniform);
    }

    fn draw(&self, pipeline: &Self::Pipeline, render_pass: &mut wgpu::RenderPass<'_>) -> bool {
        pipeline.draw(render_pass);

        false
    }
}

struct KindlePipeline {
    format: iced::wgpu::TextureFormat,
    inner: Option<KindlePipelineInner>,
}

struct KindlePipelineInner {
    screen_texture: wgpu::Texture,
    screen_texture_bind_group: wgpu::BindGroup,
    standard_uniform_buffer: wgpu::Buffer,
    standard_uniform_bind_group: wgpu::BindGroup,
    render_pipeline: wgpu::RenderPipeline,
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct StandardUniform {
    screen_size: [f32; 2],
    kindle_size: [f32; 2],
}

impl shader::Pipeline for KindlePipeline {
    fn new(
        _device: &iced::wgpu::Device,
        _queue: &iced::wgpu::Queue,
        format: iced::wgpu::TextureFormat,
    ) -> Self
    where
        Self: Sized,
    {
        Self {
            format,
            inner: None,
        }
    }
}

impl KindlePipeline {
    fn init(
        &mut self,
        device: &iced::wgpu::Device,
        display_width: u32,
        display_height: u32,
        standard_uniform: StandardUniform,
    ) {
        let screen_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Screen Texture"),
            size: wgpu::Extent3d {
                width: display_width,
                height: display_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let screen_texture_view =
            screen_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let screen_texture_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let screen_texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Screen Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let screen_texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Screen Bind Group"),
            layout: &screen_texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&screen_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&screen_texture_sampler),
                },
            ],
        });

        let standard_uniform_buffer =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Standard Buffer"),
                contents: bytemuck::cast_slice(&[standard_uniform]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        let standard_uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Standard Bind Group Layout"),
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
            });

        let standard_uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Standard Bind Group"),
            layout: &standard_uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: standard_uniform_buffer.as_entire_binding(),
            }],
        });

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[
                    &screen_texture_bind_group_layout,
                    &standard_uniform_bind_group_layout,
                ],
                ..Default::default()
            });

        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: self.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });

        self.inner = Some(KindlePipelineInner {
            screen_texture,
            screen_texture_bind_group,
            standard_uniform_buffer,
            standard_uniform_bind_group,
            render_pipeline,
        });
    }

    fn update_standard(&self, queue: &wgpu::Queue, standard_uniform: StandardUniform) {
        let Some(inner) = &self.inner else {
            return;
        };

        queue.write_buffer(
            &inner.standard_uniform_buffer,
            0,
            bytemuck::cast_slice(&[standard_uniform]),
        );
    }

    fn update_screen(
        &self,
        queue: &wgpu::Queue,
        display_width: u32,
        display_height: u32,
        screen: &[u8],
    ) {
        let Some(inner) = &self.inner else {
            return;
        };

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &inner.screen_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            screen,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(display_width),
                rows_per_image: Some(display_height),
            },
            wgpu::Extent3d {
                width: display_width,
                height: display_height,
                depth_or_array_layers: 1,
            },
        );
    }

    fn draw(&self, render_pass: &mut wgpu::RenderPass<'_>) {
        let Some(inner) = &self.inner else {
            return;
        };

        render_pass.set_pipeline(&inner.render_pipeline);
        render_pass.set_bind_group(0, &inner.screen_texture_bind_group, &[]);
        render_pass.set_bind_group(1, &inner.standard_uniform_bind_group, &[]);
        render_pass.draw(0..3, 0..1);
    }
}
