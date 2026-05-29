# Vulkan Voxel Shader Pipeline

These HLSL shaders define the intended custom renderer path for the desktop Vulkan backend:

- `voxel_visibility_buffer.hlsl` writes compact material and primitive data.
- `voxel_deferred_lighting.hlsl` resolves visibility-buffer data into lit color.
- `voxel_ibl.hlsl` contains diffuse/specular image-based-lighting helpers.

Bevy 0.18's built-in material pipeline is WGSL/wgpu-first. These HLSL files are source assets for an offline HLSL-to-SPIR-V pipeline and a future custom Bevy render graph node.

The current game intentionally renders terrain through Bevy forward PBR with depth/normal prepasses. Full `DeferredPrepass` was removed from the runtime camera because enabling it without a complete custom lighting resolve caused the world surface to render nearly black. The HLSL visibility/deferred path should be wired in only when the render graph owns the full visibility target, material buffer, cascade shadow bindings, and IBL cubemaps.
