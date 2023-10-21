![fractal_sugar](res/fractal_sugar.ico)
# fractal_sugar

### About the project
**fractal_sugar** is an experimental audio visualizer combining fractals and particle simulations. It is **cross-platform**, written in **Rust**, and uses the library [Vulkano](https://github.com/vulkano-rs/vulkano) to interact with the Vulkan API.
3D fractals are rendered using the technique of [ray-marching](http://blog.hvidtfeldts.net/index.php/2011/06/distance-estimated-3d-fractals-part-i/).
Particle physics are simulated using compute shaders.
The open source library [CPAL](https://github.com/rustaudio/cpal) is used to retrieve the audio stream and a fast Fourier transform is applied on the signal using [RustFFT](https://github.com/ejmahler/RustFFT).

### Lineage of previous projects
**fractal_sugar** is a merger and re-implementation of several of my previous OpenGL/Vulkan audio visualizers written in **F#**:
* [ColouredSugar](https://github.com/ryco117/ColouredSugar)
* [FractalDimension](https://github.com/ryco117/FractalDimension)
* [FractalDimension-Vulkan](https://github.com/ryco117/FractalDimension-Vulkan)

### Demo
##### deadmau5 Demo
[![deadmau5 Demo](https://img.youtube.com/vi/UiJ_785hC60/0.jpg)](https://www.youtube.com/watch?v=UiJ_785hC60 "deadmau5 Demo")

### Controls
| Key | Action |
|:-:|----------|
| **App-Window** | - |
| F11 | Toggle window fullscreen |
| ESC | If fullscreen, then enter windowed mode. Else, close the application |
| ENTER | *Only Windows release builds:* Toggle the visibility of the output command prompt |
| **Overlay-Window** | - |
| F1 | Toggle visibility of this Help window |
| C | Toggle visibility of the App Config window |
| **Audio** | - |
| R | Toggle the application's responsiveness to system audio |
| **Visuals** | - |
| SPACE | Toggle kaleidoscope effect on fractals |
| J | Toggle 'jello' effect on particles (i.e., the fixing of particles to a position with spring tension) |
| P | Toggle the rendering and updating of particles |
| H | Toggles whether to hide stationary particles |
| CAPS | Toggle negative-color effect for particles |
| D | Toggle between 2D and 3D projections of the particles |
| TAB | Cycle through particle color schemes. *Requires that all overlay windows are closed* |
| 0 | Select the 'empty' fractal |
| 1-6 | Select the fractal corresponding to the respective key |
| MOUSE-BTTN | Holding the primary or secondary mouse button applies a repulsive or attractive force, respectively, at the cursor's position |
| MOUSE-SCRL | Scrolling up or down changes the strength of the cursor's applied force |