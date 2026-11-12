use bytemuck::{Pod, Zeroable};
use std::collections::HashMap;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct PlaneVertex {
    pub position: [f32; 2],
}

impl PlaneVertex {
    pub const ATTRIBUTES: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x2];

    pub const fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct SolidVertex {
    pub position: [f32; 3],
}

impl SolidVertex {
    pub const ATTRIBUTES: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x3];

    pub const fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

pub(crate) struct Mesh {
    pub vertices: wgpu::Buffer,
    pub indices: wgpu::Buffer,
    pub index_count: u32,
}

impl Mesh {
    fn new<V: Pod>(
        device: &wgpu::Device,
        label: &'static str,
        vertices: &[V],
        indices: &[u32],
    ) -> Self {
        Self {
            vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: bytemuck::cast_slice(vertices),
                usage: wgpu::BufferUsages::VERTEX,
            }),
            indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: bytemuck::cast_slice(indices),
                usage: wgpu::BufferUsages::INDEX,
            }),
            index_count: indices.len() as u32,
        }
    }

    pub fn water(device: &wgpu::Device, detail: u32) -> Self {
        let side = detail + 1;
        let mut vertices = Vec::with_capacity((side * side) as usize);
        let mut indices = Vec::with_capacity((detail * detail * 6) as usize);
        for y in 0..=detail {
            for x in 0..=detail {
                vertices.push(PlaneVertex {
                    position: [
                        x as f32 / detail as f32 * 2. - 1.,
                        y as f32 / detail as f32 * 2. - 1.,
                    ],
                });
                if x < detail && y < detail {
                    let index = x + y * side;
                    indices.extend_from_slice(&[
                        index,
                        index + 1,
                        index + side,
                        index + side,
                        index + 1,
                        index + side + 1,
                    ]);
                }
            }
        }
        Self::new(device, "Plico water 200x200 grid", &vertices, &indices)
    }

    pub fn pool(device: &wgpu::Device) -> Self {
        const FACES: [[usize; 4]; 6] = [
            [0, 4, 2, 6],
            [1, 3, 5, 7],
            [0, 1, 4, 5],
            [2, 6, 3, 7],
            [0, 2, 1, 3],
            [4, 5, 6, 7],
        ];
        let octant = |index: usize| SolidVertex {
            position: [
                (index & 1) as f32 * 2. - 1.,
                (index & 2) as f32 - 1.,
                (index & 4) as f32 * 0.5 - 1.,
            ],
        };
        let mut vertices = Vec::with_capacity(20);
        let mut indices = Vec::with_capacity(30);
        for (face_index, face) in FACES.into_iter().enumerate() {
            // The source demo removes the original -Y face. Its vertex shader
            // maps that face to the pool's open top.
            if face_index == 2 {
                continue;
            }
            let base = vertices.len() as u32;
            vertices.extend(face.into_iter().map(octant));
            indices.extend_from_slice(&[base, base + 1, base + 2, base + 2, base + 1, base + 3]);
        }
        Self::new(device, "Plico water open pool", &vertices, &indices)
    }

    pub fn sphere(device: &wgpu::Device, detail: u32) -> Self {
        fn fixed(value: f32) -> f32 {
            value + (value - value * value) * 0.5
        }
        fn canonical(value: f32) -> u32 {
            if value == 0.0 { 0 } else { value.to_bits() }
        }
        let mut vertices: Vec<SolidVertex> = Vec::new();
        let mut indices = Vec::with_capacity((detail * detail * 8 * 3) as usize);
        let mut unique = HashMap::<[u32; 3], u32>::new();
        for octant in 0..8_u32 {
            let scale = [
                (octant & 1) as f32 * 2.0 - 1.0,
                (octant & 2) as f32 - 1.0,
                (octant & 4) as f32 * 0.5 - 1.0,
            ];
            let flip = scale[0] * scale[1] * scale[2] > 0.0;
            let mut data = Vec::with_capacity(((detail + 1) * (detail + 2) / 2) as usize);
            for i in 0..=detail {
                for j in 0..=detail - i {
                    let values = [
                        fixed(i as f32 / detail as f32),
                        fixed(j as f32 / detail as f32),
                        fixed((detail - i - j) as f32 / detail as f32),
                    ];
                    let length =
                        (values[0] * values[0] + values[1] * values[1] + values[2] * values[2])
                            .sqrt();
                    let position = [
                        values[0] / length * scale[0],
                        values[1] / length * scale[1],
                        values[2] / length * scale[2],
                    ];
                    let key = position.map(canonical);
                    let index = *unique.entry(key).or_insert_with(|| {
                        let index = vertices.len() as u32;
                        vertices.push(SolidVertex { position });
                        index
                    });
                    data.push(index);
                }
                if i > 0 {
                    for j in 0..=detail - i {
                        let previous_row =
                            (i - 1) * (detail + 1) - (i - 1) * (i - 1).saturating_sub(1) / 2;
                        let current_row = i * (detail + 1) - i * (i - 1) / 2;
                        let previous = (previous_row + j) as usize;
                        let current = (current_row + j) as usize;
                        let first = [data[previous], data[previous + 1], data[current]];
                        indices.extend(if flip {
                            [first[0], first[2], first[1]]
                        } else {
                            first
                        });
                        if i + j < detail {
                            let second = [data[current], data[previous + 1], data[current + 1]];
                            indices.extend(if flip {
                                [second[0], second[2], second[1]]
                            } else {
                                second
                            });
                        }
                    }
                }
            }
        }
        Self::new(
            device,
            "Plico water detail-10 geodesic sphere",
            &vertices,
            &indices,
        )
    }
}
