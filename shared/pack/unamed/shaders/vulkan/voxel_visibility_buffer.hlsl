struct VsInput {
    float3 position : POSITION;
    float3 normal : NORMAL;
    float2 uv : TEXCOORD0;
    uint material_id : TEXCOORD1;
};

struct VsOutput {
    float4 clip_position : SV_Position;
    float3 world_position : TEXCOORD0;
    float3 world_normal : TEXCOORD1;
    float2 uv : TEXCOORD2;
    uint material_id : TEXCOORD3;
};

cbuffer Camera : register(b0) {
    float4x4 view_proj;
    float4x4 model;
    float4x4 normal_from_model;
};

VsOutput vs_main(VsInput input) {
    VsOutput output;
    float4 world = mul(model, float4(input.position, 1.0));
    output.clip_position = mul(view_proj, world);
    output.world_position = world.xyz;
    output.world_normal = normalize(mul((float3x3)normal_from_model, input.normal));
    output.uv = input.uv;
    output.material_id = input.material_id;
    return output;
}

uint PackNormalOct(float3 n) {
    n /= (abs(n.x) + abs(n.y) + abs(n.z));
    float2 enc = n.z >= 0.0 ? n.xy : (1.0 - abs(n.yx)) * (n.xy >= 0.0 ? 1.0 : -1.0);
    uint2 p = uint2(saturate(enc * 0.5 + 0.5) * 65535.0);
    return p.x | (p.y << 16);
}

struct VisibilityOut {
    uint4 data0 : SV_Target0;
    float4 data1 : SV_Target1;
    float4 data2 : SV_Target2;
};

VisibilityOut ps_main(VsOutput input) {
    VisibilityOut output;
    output.data0 = uint4(input.material_id, PackNormalOct(normalize(input.world_normal)), asuint(input.clip_position.z), 0u);
    output.data1 = float4(input.world_position, 1.0);
    output.data2 = float4(input.uv, 0.0, 1.0);
    return output;
}
