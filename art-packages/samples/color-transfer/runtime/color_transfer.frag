#version 300 es
precision highp float;

uniform sampler2D u_input;
uniform sampler2D u_lut;

uniform float u_strength;
uniform float u_gamma;
uniform float u_exposure;
uniform float u_contrast;
uniform float u_highlights;
uniform float u_shadows;
uniform float u_whites;
uniform float u_blacks;
uniform float u_temperature;
uniform float u_tint;
uniform float u_saturation;
uniform float u_vibrance;
uniform float u_hue;
uniform float u_split_h_hue;
uniform float u_split_h_sat;
uniform float u_split_s_hue;
uniform float u_split_s_sat;
uniform float u_split_balance;
uniform float u_skin_protection;

in vec2 v_uv;
out vec4 outColor;

const float LUT_SIZE = 16.0;

float getSkinMask(vec3 color) {
    float cb = -0.1687 * color.r - 0.3313 * color.g + 0.5 * color.b + 0.5;
    float cr = 0.5 * color.r - 0.4187 * color.g - 0.0813 * color.b + 0.5;
    float cbDistance = cb - 0.42;
    float crDistance = cr - 0.59;
    float distance = sqrt(cbDistance * cbDistance * 2.0 + crDistance * crDistance * 1.5);
    return 1.0 - smoothstep(0.04, 0.15, distance);
}

vec3 rgbToHsv(vec3 color) {
    vec4 k = vec4(0.0, -1.0 / 3.0, 2.0 / 3.0, -1.0);
    vec4 p = mix(vec4(color.bg, k.wz), vec4(color.gb, k.xy), step(color.b, color.g));
    vec4 q = mix(vec4(p.xyw, color.r), vec4(color.r, p.yzx), step(p.x, color.r));
    float delta = q.x - min(q.w, q.y);
    float epsilon = 1.0e-10;
    return vec3(
        abs(q.z + (q.w - q.y) / (6.0 * delta + epsilon)),
        delta / (q.x + epsilon),
        q.x
    );
}

vec3 hsvToRgb(vec3 color) {
    vec4 k = vec4(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0);
    vec3 p = abs(fract(color.xxx + k.xyz) * 6.0 - k.www);
    return color.z * mix(k.xxx, clamp(p - k.xxx, 0.0, 1.0), color.y);
}

float getLuma(vec3 color) {
    return dot(color, vec3(0.2126, 0.7152, 0.0722));
}

vec3 sampleLut(vec3 color) {
    color = clamp(color, 0.0, 1.0);
    float blue = color.b * (LUT_SIZE - 1.0);
    float lowerSlice = floor(blue);
    float upperSlice = min(lowerSlice + 1.0, LUT_SIZE - 1.0);
    float blueMix = blue - lowerSlice;
    vec2 lowerUv = vec2(
        (lowerSlice * LUT_SIZE + color.r * (LUT_SIZE - 1.0) + 0.5) / (LUT_SIZE * LUT_SIZE),
        (color.g * (LUT_SIZE - 1.0) + 0.5) / LUT_SIZE
    );
    vec2 upperUv = vec2(
        (upperSlice * LUT_SIZE + color.r * (LUT_SIZE - 1.0) + 0.5) / (LUT_SIZE * LUT_SIZE),
        (color.g * (LUT_SIZE - 1.0) + 0.5) / LUT_SIZE
    );
    return mix(texture(u_lut, lowerUv).rgb, texture(u_lut, upperUv).rgb, blueMix);
}

vec3 adjustTone(vec3 color) {
    color *= pow(2.0, u_exposure);
    float brightness = getLuma(color);
    float shadowFactor = 1.0 + (u_shadows / 100.0) * (1.0 - smoothstep(0.0, 0.5, brightness));
    float highlightFactor = 1.0 + (u_highlights / 100.0) * smoothstep(0.5, 1.0, brightness);
    color *= shadowFactor * highlightFactor;
    float whites = u_whites / 100.0;
    float blacks = u_blacks / 100.0;
    color = (color - blacks * 0.2) / (1.0 + whites * 0.2 - blacks * 0.2);
    color = pow(max(color, 0.0), vec3(1.0 / max(u_gamma, 0.001)));
    float contrast = 1.0 + u_contrast / 100.0;
    return clamp((color - 0.5) * contrast + 0.5, 0.0, 1.0);
}

vec3 adjustColor(vec3 color) {
    float temperature = u_temperature / 100.0;
    float tint = u_tint / 100.0;
    color.r += temperature * 0.2;
    color.b -= temperature * 0.2;
    color.g += tint * 0.2;
    vec3 hsv = rgbToHsv(clamp(color, 0.0, 1.0));
    hsv.x = fract(hsv.x + u_hue / 360.0);
    float vibrance = u_vibrance / 100.0;
    hsv.y *= 1.0 + vibrance * (1.0 - hsv.y);
    hsv.y *= 1.0 + u_saturation / 100.0;
    hsv.y = clamp(hsv.y, 0.0, 1.0);
    return hsvToRgb(hsv);
}

vec3 applySplitTone(vec3 color) {
    if (u_split_h_sat <= 0.0 && u_split_s_sat <= 0.0) {
        return color;
    }
    float brightness = getLuma(color);
    vec3 highlightColor = hsvToRgb(vec3(u_split_h_hue / 360.0, u_split_h_sat / 100.0, 1.0));
    vec3 shadowColor = hsvToRgb(vec3(u_split_s_hue / 360.0, u_split_s_sat / 100.0, 1.0));
    float balance = 0.5 + u_split_balance / 200.0;
    float amount = smoothstep(0.0, 1.0, brightness - balance + 0.5);
    vec3 toned = mix(shadowColor * color, highlightColor * color, amount);
    return mix(color, toned, 0.3);
}

void main() {
    vec4 source = texture(u_input, v_uv);
    vec3 original = source.rgb;
    vec3 styled = sampleLut(original);
    if (u_skin_protection > 0.5) {
        styled = mix(styled, original, getSkinMask(original) * 0.85);
    }
    vec3 color = mix(original, styled, clamp(u_strength / 100.0, 0.0, 1.0));
    color = adjustTone(color);
    color = adjustColor(color);
    color = applySplitTone(color);
    outColor = vec4(color, source.a);
}
