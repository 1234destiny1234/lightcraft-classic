## Fixes

- **H.264 video exports keep LUT profiles and feathered masks:** with the GPU on, frames of a clip using a LUT profile (or a mask with Refine Edges or Feather) were graded without them. Those frames now grade on the CPU, as stills already did, so the movie matches the preview.
