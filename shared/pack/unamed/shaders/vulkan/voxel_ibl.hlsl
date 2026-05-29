struct IblSample {
    float3 diffuse;
    float3 specular;
};

float RadicalInverseVdc(uint bits) {
    bits = (bits << 16u) | (bits >> 16u);
    bits = ((bits & 0x55555555u) << 1u) | ((bits & 0xAAAAAAAAu) >> 1u);
    bits = ((bits & 0x33333333u) << 2u) | ((bits & 0xCCCCCCCCu) >> 2u);
    bits = ((bits & 0x0F0F0F0Fu) << 4u) | ((bits & 0xF0F0F0F0u) >> 4u);
    bits = ((bits & 0x00FF00FFu) << 8u) | ((bits & 0xFF00FF00u) >> 8u);
    return float(bits) * 2.3283064365386963e-10;
}

float2 Hammersley(uint i, uint sample_count) {
    return float2(float(i) / float(sample_count), RadicalInverseVdc(i));
}

float3 FresnelSchlick(float cos_theta, float3 f0) {
    return f0 + (1.0 - f0) * pow(saturate(1.0 - cos_theta), 5.0);
}

float DistributionGgx(float3 n, float3 h, float roughness) {
    float a = roughness * roughness;
    float a2 = a * a;
    float ndoth = saturate(dot(n, h));
    float denom = ndoth * ndoth * (a2 - 1.0) + 1.0;
    return a2 / max(3.14159265 * denom * denom, 0.0001);
}

float GeometrySchlickGgx(float ndotv, float roughness) {
    float r = roughness + 1.0;
    float k = (r * r) / 8.0;
    return ndotv / max(ndotv * (1.0 - k) + k, 0.0001);
}

float GeometrySmith(float3 n, float3 v, float3 l, float roughness) {
    return GeometrySchlickGgx(saturate(dot(n, v)), roughness)
        * GeometrySchlickGgx(saturate(dot(n, l)), roughness);
}

IblSample SampleIbl(TextureCube diffuse_irradiance, TextureCube specular_prefilter, SamplerState linear_sampler, float3 n, float3 v, float roughness, float3 f0) {
    float3 r = reflect(-v, n);
    float3 f = FresnelSchlick(saturate(dot(n, v)), f0);

    IblSample result;
    result.diffuse = diffuse_irradiance.Sample(linear_sampler, n).rgb * (1.0 - f);
    result.specular = specular_prefilter.SampleLevel(linear_sampler, r, roughness * 7.0).rgb * f;
    return result;
}
