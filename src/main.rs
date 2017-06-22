#[macro_use]
extern crate vulkano;
#[macro_use]
extern crate vulkano_shader_derive;
extern crate rayon;

use rayon::prelude::*;
use std::sync::Arc;
use vulkano::command_buffer::{CommandBuffer, CommandBufferBuilder};
use vulkano::image::traits::Image;
use vulkano::sync::GpuFuture;

struct WorkerDevice<'a> {
    physical_device: vulkano::instance::PhysicalDevice<'a>,
    device: Arc<vulkano::device::Device>,
    queue: Arc<vulkano::device::Queue>,
    input_buffer: std::sync::Arc<vulkano::buffer::CpuAccessibleBuffer<shader::ty::InputData>>,
    output_buffer: std::sync::Arc<vulkano::buffer::CpuAccessibleBuffer<[u32]>>,
    pipeline: Arc<vulkano::pipeline::ComputePipeline<vulkano::descriptor::pipeline_layout::PipelineLayout<shader::Layout>>>,
    set: std::sync::Arc<vulkano::descriptor::descriptor_set::SimpleDescriptorSet<(((), vulkano::descriptor::descriptor_set::SimpleDescriptorSetBuf<std::sync::Arc<vulkano::buffer::CpuAccessibleBuffer<shader::ty::InputData>>>), vulkano::descriptor::descriptor_set::SimpleDescriptorSetBuf<std::sync::Arc<vulkano::buffer::CpuAccessibleBuffer<[u32]>>>)>>,
}

impl<'a> WorkerDevice<'a> {
    fn new(physical_device: vulkano::instance::PhysicalDevice<'a>) -> Self {
        // possibly filter out the queue with required features
        let queue = physical_device.queue_families().next().expect("Could not find any queue.");
        let (device, mut queues) = vulkano::device::Device::new(&physical_device,
                                                                physical_device.supported_features(),
                                                                &vulkano::device::DeviceExtensions::none(),
                                                                [(queue, 0.5)].iter().cloned())
            .expect("failed to create device");
        let queue = queues.next().unwrap();
        let input_buffer = vulkano::buffer::cpu_access::CpuAccessibleBuffer::<shader::ty::InputData>
                                   ::from_data(device.clone(), vulkano::buffer::BufferUsage::all(), Some(queue.family()), 
                                    shader::ty::InputData {
                                        input_vec: [10, 20]
                                    })
            .expect("failed to create input buffer");
        let output_buffer = vulkano::buffer::cpu_access::CpuAccessibleBuffer::from_iter(device.clone(), vulkano::buffer::BufferUsage::all(), Some(queue.family()),
                                   (0 .. 65536u32).map(|n| n))
            .expect("failed to create output buffer");
        let shader = shader::Shader::load(&device).expect("Derp.");
        let entry_point = shader.main_entry_point();
        let pipeline = Arc::new(vulkano::pipeline::ComputePipeline::new(device.clone(), &entry_point, &()).unwrap());
        let set = Arc::new(simple_descriptor_set!(pipeline.clone(), 0, {
            input_data: input_buffer.clone(),
            output_data: output_buffer.clone(),
        }));

        WorkerDevice {
            physical_device,
            device,
            queue,
            input_buffer,
            output_buffer,
            pipeline,
            set,
        }
    }
}

fn main() {
    let application_info = vulkano::instance::ApplicationInfo::from_cargo_toml();
    let instance = vulkano::instance::Instance::new(
        Some(&application_info),
        &vulkano::instance::InstanceExtensions::supported_by_core().expect("Could not retrieve available instance extensions."),
        None
    ).expect("Could not create a Vulkano instance.");
    let mut devices: Vec<WorkerDevice> = Vec::new();

    for physical_device in vulkano::instance::PhysicalDevice::enumerate(&instance) {
        devices.push(WorkerDevice::new(physical_device));
    }

    devices.par_iter_mut().for_each(|device| {
        {
            let mut buffer_content = device.input_buffer.write().unwrap();
            buffer_content.input_vec = [buffer_content.input_vec[0] + 1, buffer_content.input_vec[1] + 2];
        }

        let command_buffer = vulkano::command_buffer::AutoCommandBufferBuilder::new(
            device.device.clone(), device.queue.family()
        ).unwrap().dispatch(
            [1, 1, 1],  // global workgroup dimensions
            device.pipeline.clone(),
            device.set.clone(),
            (),  // push constants
        ).unwrap()
        .build().unwrap();

        let future = vulkano::sync::now(device.device.clone())
            .then_execute(device.queue.clone(), command_buffer).unwrap()
            .then_signal_fence_and_flush().unwrap();

        future.wait(None).unwrap();

        let output = device.output_buffer.read().expect("could not lock the output buffer");

        for i in 0 .. 3u32 {
            println!("{:?}", output[i as usize]);
        }
    })
}

// TODO: Wait for a new release of vulkano and vulkano_shader_derive to use `path` instead of
// `src`.
mod shader {
    #[derive(VulkanoShader)]
    #[ty = "compute"]
    #[path = "shader/shader.comp"]
    struct Dummy;
}
