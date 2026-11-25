// otter.hlsl.v1: host supplies named textures, scalar uniforms and linear/clamp sampler.
// Output stays straight RGBA, including RGB under alpha zero. No implicit gamma transform.
static const float LUT_SIZE = 16.0;

float getSkinMask(float3 color) {
    float cb = -0.1687 * color.r - 0.3313 * color.g + 0.5 * color.b + 0.5;
    float cr = 0.5 * color.r - 0.4187 * color.g - 0.0813 * color.b + 0.5;
    float cbDistance = cb - 0.42;
    float crDistance = cr - 0.59;
    float distance = sqrt(cbDistance * cbDistance * 2.0 + crDistance * crDistance * 1.5);
    return 1.0 - smoothstep(0.04, 0.15, distance);
}

float3 rgbToHsv(float3 color) {
    float4 k = float4(0.0, -1.0 / 3.0, 2.0 / 3.0, -1.0);
    float4 p = lerp(float4(color.bg, k.wz), float4(color.gb, k.xy), step(color.b, color.g));
    float4 q = lerp(float4(p.xyw, color.r), float4(color.r, p.yzx), step(p.x, color.r));
    float delta = q.x - min(q.w, q.y);
    float epsilon = 1.0e-10;
    return float3(abs(q.z + (q.w - q.y) / (6.0 * delta + epsilon)), delta / (q.x + epsilon), q.x);
}

float3 hsvToRgb(float3 color) {
    float4 k = float4(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0);
    float3 p = abs(frac(color.xxx + k.xyz) * 6.0 - k.www);
    return color.z * lerp(k.xxx, clamp(p - k.xxx, 0.0, 1.0), color.y);
}

float getLuma(float3 color) {
    return dot(color, float3(0.2126, 0.7152, 0.0722));
}

float3 sampleLut(float3 color) {
    color = clamp(color, 0.0, 1.0);
    float blue = color.b * (LUT_SIZE - 1.0);
    float lowerSlice = floor(blue);
    float upperSlice = min(lowerSlice + 1.0, LUT_SIZE - 1.0);
    float blueMix = blue - lowerSlice;
    float2 lowerUv = float2(
        (lowerSlice * LUT_SIZE + color.r * (LUT_SIZE - 1.0) + 0.5) / (LUT_SIZE * LUT_SIZE),
        (color.g * (LUT_SIZE - 1.0) + 0.5) / LUT_SIZE);
    float2 upperUv = float2(
        (upperSlice * LUT_SIZE + color.r * (LUT_SIZE - 1.0) + 0.5) / (LUT_SIZE * LUT_SIZE),
        (color.g * (LUT_SIZE - 1.0) + 0.5) / LUT_SIZE);
    return lerp(u_lut.Sample(otter_sampler, lowerUv).rgb, u_lut.Sample(otter_sampler, upperUv).rgb, blueMix);
}

float3 adjustTone(float3 color) {
    color *= pow(2.0, u_exposure);
    float brightness = getLuma(color);
    float shadowFactor = 1.0 + (u_shadows / 100.0) * (1.0 - smoothstep(0.0, 0.5, brightness));
    float highlightFactor = 1.0 + (u_highlights / 100.0) * smoothstep(0.5, 1.0, brightness);
    color *= shadowFactor * highlightFactor;
    float whites = u_whites / 100.0;
    float blacks = u_blacks / 100.0;
    color = (color - blacks * 0.2) / (1.0 + whites * 0.2 - blacks * 0.2);
    color = pow(max(color, 0.0), 1.0 / max(u_gamma, 0.001));
    float contrast = 1.0 + u_contrast / 100.0;
    return clamp((color - 0.5) * contrast + 0.5, 0.0, 1.0);
}

float3 adjustColor(float3 color) {
    float temperature = u_temperature / 100.0;
    float tint = u_tint / 100.0;
    color.r += temperature * 0.2;
    color.b -= temperature * 0.2;
    color.g += tint * 0.2;
    float3 hsv = rgbToHsv(clamp(color, 0.0, 1.0));
    hsv.x = frac(hsv.x + u_hue / 360.0);
    float vibrance = u_vibrance / 100.0;
    hsv.y *= 1.0 + vibrance * (1.0 - hsv.y);
    hsv.y *= 1.0 + u_saturation / 100.0;
    hsv.y = clamp(hsv.y, 0.0, 1.0);
    return hsvToRgb(hsv);
}

float3 applySplitTone(float3 color) {
    if (u_split_h_sat <= 0.0 && u_split_s_sat <= 0.0) { return color; }
    float brightness = getLuma(color);
    float3 highlightColor = hsvToRgb(float3(u_split_h_hue / 360.0, u_split_h_sat / 100.0, 1.0));
    float3 shadowColor = hsvToRgb(float3(u_split_s_hue / 360.0, u_split_s_sat / 100.0, 1.0));
    float balance = 0.5 + u_split_balance / 200.0;
    float amount = smoothstep(0.0, 1.0, brightness - balance + 0.5);
    float3 toned = lerp(shadowColor * color, highlightColor * color, amount);
    return lerp(color, toned, 0.3);
}

float4 otter_main(float2 uv) {
    float4 source = u_input.Sample(otter_sampler, uv);
    float3 original = source.rgb;
    float3 styled = sampleLut(original);
    if (u_skin_protection > 0.5) {
        styled = lerp(styled, original, getSkinMask(original) * 0.85);
    }
    float3 color = lerp(original, styled, clamp(u_strength / 100.0, 0.0, 1.0));
    color = adjustTone(color);
    color = adjustColor(color);
    color = applySplitTone(color);
    return float4(color, source.a);
}
