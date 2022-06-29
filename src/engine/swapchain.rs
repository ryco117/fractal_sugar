use std::sync::Arc;

use vulkano::device::physical::PhysicalDevice;
use vulkano::device::Device;
use vulkano::format::Format;
use vulkano::image::{ImageUsage, SwapchainImage};
use vulkano::swapchain::{
    PresentMode, Surface, SurfaceInfo, Swapchain, SwapchainCreateInfo, SwapchainCreationError,
};
use winit::dpi::PhysicalSize;
use winit::window::Window;

pub struct EngineSwapchain {
    swapchain: Arc<Swapchain<Window>>,
    images: Vec<Arc<SwapchainImage<Window>>>,
}

pub enum RecreateSwapchainResult {
    Success,
    ExtentNotSupported,
}

impl EngineSwapchain {
    pub fn new(
        physical_device: PhysicalDevice,
        device: Arc<Device>,
        surface: Arc<Surface<Window>>,
        desired_present_mode: PresentMode,
    ) -> Self {
        // Determine what features our surface can support
        let surface_capabilities = physical_device
            .surface_capabilities(&surface, SurfaceInfo::default())
            .expect("Failed to get surface capabilities");

        // Determine properties of surface (on this physical device)
        let dimensions = surface.window().inner_size();
        let composite_alpha = surface_capabilities
            .supported_composite_alpha
            .iter()
            .next()
            .unwrap();
        let image_format = {
            let desired_formats = [Format::R8G8B8A8_UNORM, Format::B8G8R8A8_UNORM];
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

        // Get preferred present mode with fallback to FIFO (which any Vulkan instance must support)
        let present_mode = {
            if physical_device
                .surface_present_modes(&surface)
                .unwrap()
                .any(|p| p == desired_present_mode)
            {
                desired_present_mode
            } else {
                println!("Fallback to default present mode FIFO");
                PresentMode::Fifo
            }
        };

        // Attempt to create one more image buffer than the minimum required, but constrained by optional maximum count
        let image_count = {
            let desired_count = surface_capabilities.min_image_count + 1;
            let max_count = surface_capabilities.max_image_count.unwrap_or(0);
            if max_count > 0 {
                std::cmp::min(desired_count, max_count)
            } else {
                desired_count
            }
        };

        // Create new swapchain with specified properties
        let (swapchain, images) = Swapchain::new(
            device,
            surface,
            SwapchainCreateInfo {
                min_image_count: image_count, // Use one more buffer than the minimum in swapchain
                image_format: Some(image_format),
                image_extent: dimensions.into(),
                image_usage: {
                    // Swapchain images are going to be used for color, as well as MSAA destination
                    let mut u = ImageUsage::color_attachment();
                    u.transfer_destination = true;
                    u
                },
                composite_alpha,
                present_mode,
                ..Default::default()
            },
        )
        .unwrap();

        Self { swapchain, images }
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

                RecreateSwapchainResult::Success
            }

            // This error tends to happen when the user is manually resizing the window.
            // Simply restarting the loop is the easiest way to fix this issue
            Err(SwapchainCreationError::ImageExtentNotSupported { .. }) => {
                RecreateSwapchainResult::ExtentNotSupported
            }

            // Unexpected error
            Err(e) => panic!("Failed to recreate swapchian: {:?}", e),
        }
    }

    // Swapchain getters
    pub fn swapchain(&self) -> Arc<Swapchain<Window>> {
        self.swapchain.clone()
    }
    pub fn images(&self) -> Vec<Arc<SwapchainImage<Window>>> {
        self.images.clone()
    }
    pub fn image_format(&self) -> Format {
        self.swapchain.image_format()
    }
}
