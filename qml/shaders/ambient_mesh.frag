#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    vec4 baseTop;
    vec4 baseBottom;
    vec4 light1Color;
    vec4 light2Color;
    vec4 borderColor;
    float qt_Opacity;
    float radius;
    float windowWidth;
    float windowHeight;
    float light1Strength;
    float light2Strength;
    float grainStrength;
    float borderWidth;
};

// Subtle organic noise for paper/matte porcelain texture and anti-banding
float pseudoNoise(vec2 coord)
{
    vec2 p = coord * 1.61803398875;
    float n = sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453123;
    return fract(n) - 0.5;
}

void main()
{
    vec2 size = vec2(max(windowWidth, 1.0), max(windowHeight, 1.0));
    vec2 pixel = qt_TexCoord0 * size;
    vec2 uv = qt_TexCoord0;

    // Corner mask via SDF
    float curve = min(radius, min(size.x, size.y) * 0.5);
    vec2 halfSize = size * 0.5;
    vec2 delta = abs(pixel - halfSize) - (halfSize - vec2(curve));
    float dist = length(max(delta, vec2(0.0))) + min(max(delta.x, delta.y), 0.0) - curve;
    float windowMask = 1.0 - smoothstep(-1.0, 0.0, dist);

    // 1. Base gradient (vertical with gentle curve)
    float baseT = smoothstep(0.0, 1.0, uv.y);
    vec3 color = mix(baseTop.rgb, baseBottom.rgb, baseT);

    // 2. Light 1: Top-Left atmospheric warm/morning glow
    float d1 = distance(uv, vec2(-0.05, -0.05));
    float glow1 = smoothstep(1.05, 0.0, d1);
    color = mix(color, light1Color.rgb, glow1 * light1Strength);

    // 3. Light 2: Bottom-Right deep mineral/sediment glow
    float d2 = distance(uv, vec2(1.05, 1.05));
    float glow2 = smoothstep(1.15, 0.0, d2);
    color = mix(color, light2Color.rgb, glow2 * light2Strength);

    // 4. Inset Top Sheen (subtle light reflection along the top window edge)
    if (borderWidth > 0.0) {
        float sheenDist = abs(dist + borderWidth * 0.5);
        float topEdgeFactor = 1.0 - smoothstep(0.0, 6.0, pixel.y);
        float sheen = (1.0 - smoothstep(0.0, 1.2, sheenDist)) * topEdgeFactor * 0.12;
        color += vec3(sheen);
    }

    // 5. Fine organic grain (消除数码色阶条纹，赋予宣纸与哑光瓷器的温润触感)
    float noise = pseudoNoise(pixel);
    color += noise * grainStrength;

    // 6. 1px Inset Border
    if (borderWidth > 0.0) {
        float bDist = dist + borderWidth;
        float borderAlpha = smoothstep(-1.0, 0.0, dist) - smoothstep(-1.0, 0.0, bDist);
        color = mix(color, borderColor.rgb, clamp(borderAlpha * borderColor.a, 0.0, 1.0));
    }

    fragColor = vec4(clamp(color, 0.0, 1.0), 1.0) * windowMask * qt_Opacity;
}
