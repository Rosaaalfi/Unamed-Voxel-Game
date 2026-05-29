#include "voxel_ibl.hlsl"

Texture2D<uint4> visibility_data : register(t0);
Texture2D<float4> position_data : register(t1);
Texture2D<float4> attribute_data : register(t2);
Texture2DArray material_albedo : register(t3);
TextureCube diffuse_irradiance : register(t4);
TextureCube specular_prefilter : register(t5);
Texture2DArray<float> cascade_shadow_maps : register(t6);
SamplerState nearest_sampler : register(s0);
SamplerState linear_sampler : register(s1);
SamplerComparisonState shadow_sampler : register(s2);

cbuffer Lighting : register(b0) {
    float3 camera_position;
    float roughness;
    float3 sun_direction;
    float metallic;
    float3 sun_color;
    float sun_intensity;
    float3 ambient_color;
    float ambient_intensity;
    float4x4 light_view_proj[4];
    float4 cascade_far;
    float shadow_strength;
    float3 _padding0;
};

struct PsInput {
    float4 clip_position : SV_Position;
    float2 uv : TEXCOORD0;
};

float3 UnpackNormalOct(uint packed) {
    float2 f = float2(packed & 65535u, packed >> 16) / 65535.0 * 2.0 - 1.0;
    float3 n = float3(f.x, f.y, 1.0 - abs(f.x) - abs(f.y));
    float t = saturate(-n.z);
    n.xy += n.xy >= 0.0 ? -t : t;
    return normalize(n);
}

float4 ps_main(PsInput input) : SV_Target0 {
    uint2 pixel = uint2(input.clip_position.xy);
    uint4 vis = visibility_data.Load(uint3(pixel, 0));
    float3 world_position = position_data.Load(uint3(pixel, 0)).xyz;
    float2 material_uv = attribute_data.Load(uint3(pixel, 0)).xy;

    uint material_id = vis.x;
    float3 n = UnpackNormalOct(vis.y);
    float3 v = normalize(camera_position - world_position);
    float3 l = normalize(-sun_direction);
    float3 h = normalize(v + l);

    float3 albedo = material_albedo.Sample(nearest_sampler, float3(material_uv, material_id)).rgb;
    float3 f0 = lerp(float3(0.04, 0.04, 0.04), albedo, metallic);
    float3 f = FresnelSchlick(saturate(dot(h, v)), f0);
    float d = DistributionGgx(n, h, roughness);
    float g = GeometrySmith(n, v, l, roughness);
    float ndotl = saturate(dot(n, l));
    float3 specular = (d * g * f) / max(4.0 * saturate(dot(n, v)) * ndotl, 0.0001);
    float3 diffuse = (1.0 - f) * (1.0 - metallic) * albedo / 3.14159265;

    IblSample ibl = SampleIbl(diffuse_irradiance, specular_prefilter, linear_sampler, n, v, roughness, f0);
    float view_depth = asfloat(vis.z);
    uint cascade_index = view_depth > cascade_far.x ? (view_depth > cascade_far.y ? (view_depth > cascade_far.z ? 3u : 2u) : 1u) : 0u;
    float4 light_clip = mul(light_view_proj[cascade_index], float4(world_position, 1.0));
    float3 light_ndc = light_clip.xyz / max(light_clip.w, 0.0001);
    float2 shadow_uv = light_ndc.xy * float2(0.5, -0.5) + 0.5;
    float shadow_depth = light_ndc.z;
    float shadow = cascade_shadow_maps.SampleCmpLevelZero(shadow_sampler, float3(shadow_uv, cascade_index), shadow_depth);
    shadow = lerp(1.0, shadow, shadow_strength);

    float3 direct = (diffuse + specular) * sun_color * sun_intensity * ndotl * shadow;
    float3 indirect = (ibl.diffuse * albedo + ibl.specular) * ambient_color * ambient_intensity;

    return float4(direct + indirect, 1.0);
}
