#[macro_use]
extern crate vulkano;
#[macro_use]
extern crate vulkano_shader_derive;
extern crate rayon;
extern crate rand;
extern crate secp256k1;

mod context;

use rayon::prelude::*;
use std::sync::Arc;
use std::rc::Rc;
use vulkano::command_buffer::CommandBuffer;
use vulkano::command_buffer::submit::SubmitCommandBufferBuilder;
use vulkano::image::traits::Image;
use vulkano::sync::GpuFuture;
use rand::os::OsRng;
use rand::Rng;
use secp256k1::{Secp256k1, ContextFlag};
use secp256k1::key::SecretKey;
use context::Secp256k1Context;

const SECRET_KEY_INT_ARRAY_LENGTH: usize = 8;

struct WorkerDevice<'a> {
    physical_device: vulkano::instance::PhysicalDevice<'a>,
    device: Arc<vulkano::device::Device>,
    max_invocations: usize,
    queue: Arc<vulkano::device::Queue>,
    input_buffer: std::sync::Arc<vulkano::buffer::CpuAccessibleBuffer<[u32]>>,
    output_buffer: std::sync::Arc<vulkano::buffer::CpuAccessibleBuffer<[u32]>>,
    pipeline: Arc<vulkano::pipeline::ComputePipeline<vulkano::descriptor::pipeline_layout::PipelineLayout<shader::Layout>>>,
    set: std::sync::Arc<vulkano::descriptor::descriptor_set::SimpleDescriptorSet<(((), vulkano::descriptor::descriptor_set::SimpleDescriptorSetBuf<std::sync::Arc<vulkano::buffer::CpuAccessibleBuffer<[u32]>>>), vulkano::descriptor::descriptor_set::SimpleDescriptorSetBuf<std::sync::Arc<vulkano::buffer::CpuAccessibleBuffer<[u32]>>>)>>,
}

impl<'a> WorkerDevice<'a> {
    fn new(physical_device: vulkano::instance::PhysicalDevice<'a>, secp256k1_context: Rc<Secp256k1Context>) -> Self {
        // possibly filter out the queue with required features
        let queue = physical_device.queue_families().next().expect("Could not find any queue.");
        let (device, mut queues) = vulkano::device::Device::new(&physical_device,
                                                                physical_device.supported_features(),
                                                                &vulkano::device::DeviceExtensions::none(),
                                                                [(queue, 0.5)].iter().cloned())
            .expect("failed to create device");
        let max_invocations: usize = 128;
        let queue = queues.next().unwrap();
        let context_buffer = vulkano::buffer::immutable::ImmutableBuffer::from_data(
            shader::ty::Context {
                context: secp256k1_context.as_ref().into(),
            },
            vulkano::buffer::BufferUsage::uniform_buffer(),
            Some(queue.family()),
            queue.clone(),
        ).expect("failed to create input buffer");
        let mut rng = OsRng::new().expect("Could not create a safe system random number generator.");
        let input_buffer = vulkano::buffer::cpu_access::CpuAccessibleBuffer::from_iter(device.clone(), vulkano::buffer::BufferUsage::all(), Some(queue.family()),
                                   (0 .. max_invocations * SECRET_KEY_INT_ARRAY_LENGTH).map(|_| rng.next_u32()))
            .expect("failed to create input buffer");
        let output_buffer = vulkano::buffer::cpu_access::CpuAccessibleBuffer::from_iter(device.clone(), vulkano::buffer::BufferUsage::all(), Some(queue.family()),
                                   (0 .. max_invocations).map(|_| 0))
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
            // Hardcoded for now, update with
            // https://www.khronos.org/registry/OpenGL/extensions/ARB/ARB_compute_variable_group_size.txt
            // or the Vulkan equivalent.
            max_invocations,
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
    let extensions = vulkano::instance::InstanceExtensions {
        ext_debug_report: true,  // TODO: remove debug mode
        ..vulkano::instance::InstanceExtensions::none()
    };
    let layer = "VK_LAYER_LUNARG_standard_validation";
    let layers = vec!(&layer);
    let instance = vulkano::instance::Instance::new(
        Some(&application_info),
        &extensions,
        layers,
    ).expect("Could not create a Vulkano instance.");
    let all = vulkano::instance::debug::MessageTypes {
        error: true,
        warning: true,
        performance_warning: true,
        information: true,
        debug: true,
    };

    let _debug_callback = vulkano::instance::debug::DebugCallback::new(&instance, all, |msg| {
        let ty = if msg.ty.error {
            "error"
        } else if msg.ty.warning {
            "warning"
        } else if msg.ty.performance_warning {
            "performance_warning"
        } else if msg.ty.information {
            "information"
        } else if msg.ty.debug {
            "debug"
        } else {
            panic!("no-impl");
        };
        println!("{} {}: {}", msg.layer_prefix, ty, msg.description);
    }).ok();
    let mut devices: Vec<WorkerDevice> = Vec::new();
    let secp256k1_context = Rc::new(Secp256k1Context::with_caps(ContextFlag::Full));

    for physical_device in vulkano::instance::PhysicalDevice::enumerate(&instance) {
        devices.push(WorkerDevice::new(physical_device, secp256k1_context.clone()));
    }

    let secp256k1 = Secp256k1::with_caps(ContextFlag::Full);

    devices.par_iter_mut().for_each(|device| {
        {
            // {{{ generate test input
            let mut buffer_content = [0u8; 192];
            let orderc = [
                0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
                0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xfe,
                0xba, 0xae, 0xdc, 0xe6, 0xaf, 0x48, 0xa0, 0x3b,
                0xbf, 0xd2, 0x5e, 0x8c, 0xd0, 0x36, 0x41, 0x41,
            ];

            buffer_content[0 .. 32].clone_from_slice(&orderc);

            for x in &mut buffer_content[32 .. 64] { *x = 0xFF; }

            for x in &mut buffer_content[64 .. 96] { *x = 0x00; }

            for x in &mut buffer_content[96 .. 128] { *x = 0x00; }
            buffer_content[127] = 0x01;

            buffer_content[128 .. 160].clone_from_slice(&orderc);
            buffer_content[159] = 0x42;

            buffer_content[160 .. 192].clone_from_slice(&orderc);
            buffer_content[191] = 0x40;
            // }}}

            // {{{ validate input using the original Secp256k1 implementation
            let expected_results = [ false, false, false, true, false, true ];

            for (i, expected_result) in expected_results.iter().enumerate() {
                // let u8_buffer: Vec<u8> = buffer_content[i*32 .. (i+1)*32].iter().map(|i| *i as u8).collect();
                println!("{}: {:?}", expected_result, SecretKey::from_slice(&secp256k1, &buffer_content))
            }
            // }}}

            // {{{ convert bytes to u32s
            let mut input_buffer = device.input_buffer.write().unwrap();

            for x in &mut input_buffer[0 .. 6 * 8] { *x = 0x00; }

            for key in 0 .. 6 {
                for byte in 0 .. 32 {
                    let integer = 7 - byte / 4;
                    let bitshift = (3 - byte % 4) * 8;
                    let abs_byte = key * 32 + byte;
                    let abs_integer = key * 8 + integer;
                    input_buffer[abs_integer] |= (buffer_content[abs_byte] as u32) << bitshift;
                }
            }
            // }}}
        }

        let command_buffer = vulkano::command_buffer::AutoCommandBufferBuilder::new(
            device.device.clone(), device.queue.family()
        ).unwrap().dispatch(
            [device.max_invocations as u32, 1, 1],  // global workgroup dimensions
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

        for invocation_index in 0 .. device.max_invocations {
            let array_index = invocation_index;
            // for secret_key_int_index in 0 .. SECRET_KEY_INT_ARRAY_LENGTH {
            //     let array_index = invocation_index * SECRET_KEY_INT_ARRAY_LENGTH + secret_key_int_index as usize;
                println!("{:?}", output[array_index]);
            // }
        }
    })
}

mod shader {
    #[derive(VulkanoShader)]
    #[ty = "compute"]
    #[path = "shader/shader.comp"]
    struct Dummy;
}
