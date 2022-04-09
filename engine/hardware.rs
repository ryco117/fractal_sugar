use std::sync::Arc;

use vulkano::device::{Device, DeviceCreateInfo, DeviceExtensions, Queue, QueueCreateInfo};
use vulkano::device::physical::{PhysicalDevice, PhysicalDeviceType, QueueFamily};
use vulkano::instance::Instance;
use vulkano::swapchain::Surface;
use winit::window::Window;

// Select the best physical device for performing Vulkan operations
fn select_best_physical_device<'a>(
    instance: &'a Arc<Instance>,
    surface: Arc<Surface<Window>>,
    device_extensions: &DeviceExtensions
) -> (PhysicalDevice<'a>, QueueFamily<'a>) {
    // Iterate through all devices in Vulkan instance
    PhysicalDevice::enumerate(&instance)
        // Require device contain at least our desired extensions
        .filter(|&p| p.supported_extensions().is_superset_of(&device_extensions))

        // Require device to have compatible queues and find one
        .filter_map(|p|
            p.queue_families()
                // Find first queue family supporting graphics pipeline and a surface (window).
                // If no such queue family exists, device will not be considered
                .find(|&q| q.supports_graphics() && q.supports_surface(&surface).unwrap_or(false))
                .map(|q| (p, q))
        )

        // Preference from most dedicated graphics hardware to least
        .min_by_key(|(p, _)| match p.properties().device_type {
            PhysicalDeviceType::DiscreteGpu => 0,
            PhysicalDeviceType::IntegratedGpu => 1,
            PhysicalDeviceType::VirtualGpu => 2,
            PhysicalDeviceType::Cpu => 3,
            PhysicalDeviceType::Other => 4
        })
        .expect("Could not find a compatible GPU")
}

// Retrieve resources best suited for graphical Vulkan operations
pub fn select_hardware<'a>(
    instance: &'a Arc<Instance>,
    surface: Arc<Surface<Window>>
) -> (PhysicalDevice<'a>, Arc<Device>, Arc<Queue>) {
    // Perform non-trivial search for optimal GPU and corresponding queue family
    let device_extensions = DeviceExtensions {
        khr_swapchain: true, // Require support for a swapchain
        ..DeviceExtensions::none()
    };
    let (physical_device, queue_family) = select_best_physical_device(&instance, surface, &device_extensions);

    // Pretty-print which GPU was selected
    println!(
        "Using device: {} (type: {:?})",
        physical_device.properties().device_name,
        physical_device.properties().device_type
    );

    // Create a logical Vulkan device object
    let (device, mut queues) = Device::new(
        physical_device,
        DeviceCreateInfo {
            // Here we pass the desired queue families that we want to use
            queue_create_infos: vec![QueueCreateInfo::family(queue_family)],
            enabled_extensions: physical_device
                .required_extensions()
                .union(&device_extensions),
            ..Default::default()
        }
    ).expect("Failed to create device");

    // Retrieve first device queue
    let queue = queues.next().unwrap();

    // Return new objects
    (physical_device, device, queue)
}