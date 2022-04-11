use std::sync::Arc;

use vulkano::device::Device;
use vulkano::device::physical::PhysicalDevice;
use vulkano::image::{ImageUsage, SwapchainImage};
use vulkano::swapchain::{Swapchain, PresentMode, Surface, SwapchainCreateInfo, SwapchainCreationError};
use winit::window::Window;
use winit::dpi::PhysicalSize;

pub struct EngineSwapchain {
    swapchain: Arc<Swapchain<Window>>,
    images: Vec<Arc<SwapchainImage<Window>>>
}

pub enum RecreateSwapchainResult {
    Success,
    ExtentNotSupported,
    Failure(String)
}

impl EngineSwapchain {
    pub fn new(physical_device: PhysicalDevice, device: Arc<Device>, surface: Arc<Surface<Window>>, desired_present_mode: PresentMode) -> Self {
        // Determine what features our surface can support
        let surface_capabilities = physical_device
            .surface_capabilities(&surface, Default::default())
            .expect("Failed to get surface capabilities");
        
        // Determine properties of surface (on this physical device)
        let dimensions = surface.window().inner_size();
        let composite_alpha = surface_capabilities.supported_composite_alpha.iter().next().unwrap();
        let image_format = Some(
            physical_device
                .surface_formats(&surface, Default::default())
                .unwrap()[0].0
        );

        // Get preferred present mode with fallback to FIFO (which any Vulkan instance must support)
        let present_mode = {
            if physical_device.surface_present_modes(&surface).unwrap().any(|p| p == desired_present_mode) {
                desired_present_mode
            } else {
                println!("Fallback to default present mode FIFO");
                PresentMode::Fifo
            }
        };

        // Attempt to select one more image buffer than the minimum required, but constrained by optional maximum count
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
            device.clone(),
            surface.clone(),
            SwapchainCreateInfo {
                min_image_count: image_count, // Use one more buffer than the minimum in swapchain
                image_format,
                image_extent: dimensions.into(),
                image_usage: ImageUsage::color_attachment(), // Images are going to be used for color (as opposed to depth, etc.)
                composite_alpha,
                present_mode: present_mode,
                ..Default::default()
            }
        ).unwrap();

        EngineSwapchain {
            swapchain,
            images
        }
    }

    // Recreate swapchain using new dimensions
    pub fn recreate(&mut self, new_dimensions: PhysicalSize<u32>) -> RecreateSwapchainResult {
        let old_swapchain = self.swapchain.clone();

        // Create new swapchain with desired dimensions
        let recreate_swapchain = old_swapchain.recreate(SwapchainCreateInfo {
            image_extent: new_dimensions.into(),
            ..old_swapchain.create_info()
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
            Err(SwapchainCreationError::ImageExtentNotSupported {..}) => RecreateSwapchainResult::ExtentNotSupported,

            // Unexpected error
            Err(e) => RecreateSwapchainResult::Failure(format!("{:?}", e))
        }
    }

    // Swapchain getters
    pub fn get_swapchain(&self) -> Arc<Swapchain<Window>> {self.swapchain.clone()}
    pub fn get_images(&self) -> Vec<Arc<SwapchainImage<Window>>> {self.images.clone()}
}