/*
    fractal_sugar - An experimental audio visualizer combining fractals and particle simulations.
    Copyright (C) 2022  Ryan Andersen

    This program is free software: you can redistribute it and/or modify
    it under the terms of the GNU General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    This program is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU General Public License for more details.

    You should have received a copy of the GNU General Public License
    along with this program.  If not, see <https://www.gnu.org/licenses/>.
*/

use std::sync::Arc;

use smallvec::SmallVec;
use vulkano::device::physical::{PhysicalDevice, PhysicalDeviceType};
use vulkano::device::{
    Device, DeviceCreateInfo, DeviceExtensions, Queue, QueueCreateInfo, QueueFlags,
};
use vulkano::format::Format;
use vulkano::image::{ImageUsage, SwapchainImage};
use vulkano::instance::Instance;
use vulkano::swapchain::{
    AcquireError, PresentMode, Surface, SurfaceInfo, Swapchain, SwapchainCreateInfo,
    SwapchainCreationError, SwapchainPresentInfo,
};

use vulkano::sync::GpuFuture;
use winit::dpi::PhysicalSize;
use winit::window::Window;

const MAX_EXPECTED_FRAMES_IN_FLIGHT: usize = 3;

pub struct EngineSwapchain {
    fences: SmallVec<[Option<Box<dyn GpuFuture>>; MAX_EXPECTED_FRAMES_IN_FLIGHT]>,
    images: Vec<Arc<SwapchainImage>>,
    present_index: Option<u32>,
    swapchain: Arc<Swapchain>,
}

pub enum RecreateSwapchainResult {
    Ok,
    ExtentNotSupported,
}

// Information for an acquired swapchain image.
pub struct AcquiredImageData {
    pub image_index: u32,
    pub suboptimal: bool,
    pub acquire_future: Box<dyn GpuFuture>,
}

pub trait WindowSurface {
    fn window(&self) -> &Window;
}

impl WindowSurface for Surface {
    fn window(&self) -> &Window {
        self.object().unwrap().downcast_ref::<Window>().unwrap()
    }
}

// Select the best physical device for performing Vulkan operations
fn select_best_physical_device(
    instance: &Arc<Instance>,
    surface: &Arc<Surface>,
    device_extensions: &DeviceExtensions,
) -> (Arc<PhysicalDevice>, u32) {
    // Iterate through all devices in Vulkan instance
    instance
        .enumerate_physical_devices()
        .expect("Failed to enumerate physical devices")
        // Require device contain at least our desired extensions
        .filter(|p| p.supported_extensions().contains(device_extensions))
        // Require device to have compatible queues and find one
        .filter_map(|p| {
            p.queue_family_properties()
                .iter()
                .enumerate()
                .position(|(i, q)| {
                    // Find first queue family supporting graphics pipeline and a surface (window).
                    // If no such queue family exists, device will not be considered
                    q.queue_flags.contains(QueueFlags::GRAPHICS)
                        && p.surface_support(i as u32, surface).unwrap_or(false)
                })
                .map(|q| (p, q as u32))
        })
        // Preference from most dedicated graphics hardware to least
        .min_by_key(|(p, _)| match p.properties().device_type {
            PhysicalDeviceType::DiscreteGpu => 0,
            PhysicalDeviceType::IntegratedGpu => 1,
            PhysicalDeviceType::VirtualGpu => 2,
            PhysicalDeviceType::Cpu => 3,
            _ => 4,
        })
        .expect("Could not find a compatible GPU")
}

// Retrieve resources best suited for graphical Vulkan operations
pub fn select_hardware(
    instance: &Arc<Instance>,
    surface: &Arc<Surface>,
) -> (Arc<PhysicalDevice>, Arc<Device>, Arc<Queue>) {
    // Perform non-trivial search for optimal GPU and corresponding queue family
    let device_extensions = DeviceExtensions {
        khr_swapchain: true, // Require support for a swapchain
        ..DeviceExtensions::empty()
    };
    let (physical_device, queue_family_index) =
        select_best_physical_device(instance, surface, &device_extensions);

    // Pretty-print which GPU was selected
    println!(
        "Device: {} (Type: {:?})",
        physical_device.properties().device_name,
        physical_device.properties().device_type
    );

    // Create a logical Vulkan device object
    let (device, mut queues) = Device::new(
        physical_device.clone(),
        DeviceCreateInfo {
            // Here we pass the desired queue families that we want to use
            queue_create_infos: vec![QueueCreateInfo {
                queue_family_index,
                ..Default::default()
            }],
            enabled_extensions: device_extensions,
            ..Default::default()
        },
    )
    .expect("Failed to create device");

    // Retrieve first device queue
    let queue = queues.next().unwrap();

    // Return new objects
    (physical_device, device, queue)
}

impl EngineSwapchain {
    pub fn new(
        physical_device: &Arc<PhysicalDevice>,
        device: &Arc<Device>,
        surface: Arc<Surface>,
        desired_present_mode: PresentMode,
    ) -> Self {
        // Determine what features our surface can support.
        let surface_capabilities = physical_device
            .surface_capabilities(&surface, SurfaceInfo::default())
            .expect("Failed to get surface capabilities");

        // Determine the properties of surface (on this physical device).
        let dimensions = surface.window().inner_size();
        let composite_alpha = surface_capabilities
            .supported_composite_alpha
            .into_iter()
            .next()
            .unwrap();
        let image_format = {
            let desired_formats = [
                Format::B8G8R8A8_SNORM,
                Format::R8G8B8A8_SNORM,
                Format::B8G8R8A8_UNORM,
                Format::R8G8B8A8_UNORM,
            ];
            physical_device
                .surface_formats(&surface, SurfaceInfo::default())
                .unwrap()
                .into_iter()
                .find_map(|(format, _)| {
                    if desired_formats.contains(&format) {
                        Some(format)
                    } else {
                        None
                    }
                })
                .expect("Failed to find suitable surface format")
        };
        println!("Image color-format {image_format:?}");

        // Get preferred present mode with fallback to FIFO (which any Vulkan instance must support)
        let present_mode = {
            if physical_device
                .surface_present_modes(&surface)
                .unwrap()
                .any(|p| p == desired_present_mode)
            {
                desired_present_mode
            } else {
                // The Vulkano spec requires FIFO to be supported.
                println!("Fallback to default present mode, FIFO");
                PresentMode::Fifo
            }
        };

        // Attempt to create one more image buffer than the minimum required, but constrained by the optional maximum count.
        let image_count = {
            let desired_count = surface_capabilities.min_image_count + 1;
            let max_count = surface_capabilities.max_image_count.unwrap_or(0);
            if max_count > 0 {
                std::cmp::min(desired_count, max_count)
            } else {
                desired_count
            }
        };

        // Create a new swapchain with the specified properties.
        let (swapchain, images) = Swapchain::new(
            device.clone(),
            surface,
            SwapchainCreateInfo {
                min_image_count: image_count, // Use one more buffer than the minimum in swapchain.
                image_format: Some(image_format),
                image_extent: dimensions.into(),
                image_usage: {
                    // Swapchain images are going to be used for color as well as MSAA destination.
                    ImageUsage::COLOR_ATTACHMENT | ImageUsage::TRANSFER_DST
                },
                composite_alpha,
                present_mode,
                ..Default::default()
            },
        )
        .unwrap();

        // Create a frame-in-flight fence for each image/frame buffer.
        // This maximum work to be done before needing to wait for a framebuffer's resources to become free again.
        let fences = std::iter::repeat_with(|| None).take(images.len()).collect();

        Self {
            fences,
            swapchain,
            images,
            present_index: None,
        }
    }

    // Recreate swapchain using new dimensions
    pub fn recreate(&mut self, new_dimensions: PhysicalSize<u32>) -> RecreateSwapchainResult {
        // Create new swapchain with desired dimensions
        let recreate_swapchain = self.swapchain.recreate(SwapchainCreateInfo {
            image_extent: new_dimensions.into(),
            ..self.swapchain.create_info()
        });
        match recreate_swapchain {
            // Successful re-creation of swapchain, update self
            Ok((new_swapchain, new_images)) => {
                self.swapchain = new_swapchain;
                self.images = new_images;

                RecreateSwapchainResult::Ok
            }

            // This error tends to happen when the user is manually resizing the window.
            // Simply restarting the loop is the easiest way to fix this issue
            Err(SwapchainCreationError::ImageExtentNotSupported { .. }) => {
                RecreateSwapchainResult::ExtentNotSupported
            }

            // Unexpected error
            Err(e) => panic!("Failed to recreate swapchian: {e:?}"),
        }
    }

    // Retrieve the index of the next render destination
    pub fn acquire_next_image(&mut self) -> Result<AcquiredImageData, AcquireError> {
        let (image_index, suboptimal, acquire_future) =
            vulkano::swapchain::acquire_next_image(self.swapchain.clone(), None)?;
        self.present_index = Some(image_index);

        let acquire_future = match self.fences[image_index as usize].take() {
            Some(f) => f.join(acquire_future).boxed(),
            None => acquire_future.boxed(),
        };

        Ok(AcquiredImageData {
            image_index,
            suboptimal,
            acquire_future,
        })
    }

    // Present the current swapchain index.
    pub fn present(&mut self, queue: &Arc<Queue>, future: Box<dyn GpuFuture>) -> bool {
        let image_index = self
            .present_index
            .take()
            .expect("Must acquire an image before presenting");

        // Present result to swapchain buffer
        let present_future = future
            .then_swapchain_present(
                queue.clone(),
                SwapchainPresentInfo::swapchain_image_index(self.swapchain.clone(), image_index),
            )
            // Finish synchronization.
            .then_signal_fence_and_flush();

        // Update this frame's future with the result of the current render.
        let mut requires_recreate_swapchain = false;
        self.fences[image_index as usize] = match present_future {
            // Success, store result into vector
            Ok(future) => {
                future.wait(None).unwrap();
                Some(future.boxed())
            }

            // Swapchain is out-of-date, request its recreation next frame.
            Err(vulkano::sync::FlushError::OutOfDate) => {
                requires_recreate_swapchain = true;
                None
            }

            // Unknown failure
            Err(e) => panic!("Failed to flush future: {e:?}"),
        };

        // Return whether a swapchain recreation was deemed necessary.
        requires_recreate_swapchain
    }

    // Swapchain getters
    pub fn images(&self) -> &Vec<Arc<SwapchainImage>> {
        &self.images
    }
    pub fn image_format(&self) -> Format {
        self.swapchain.image_format()
    }
    pub fn swapchain(&self) -> &Arc<Swapchain> {
        &self.swapchain
    }
}
