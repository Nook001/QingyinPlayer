#version 440

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float radius;
    float coverWidth;
    float coverHeight;
    float overlayAmount;
    float overlayR;
    float overlayG;
    float overlayB;
    float overlayA;
};

layout(binding = 1) uniform sampler2D source;

void main()
{
    vec2 size = vec2(max(coverWidth, 1.0), max(coverHeight, 1.0));
    vec2 pixel = qt_TexCoord0 * size;
    float curve = min(radius, min(size.x, size.y) * 0.5);
    vec2 halfSize = size * 0.5;
    vec2 delta = abs(pixel - halfSize) - (halfSize - vec2(curve));
    float dist = length(max(delta, vec2(0.0))) + min(max(delta.x, delta.y), 0.0) - curve;
    // Feather inside the geometric corner so the photo does not extend past the scrim.
    float mask = 1.0 - smoothstep(-1.0, 0.0, dist);

    vec4 color = texture(source, qt_TexCoord0);
    float scrimA = overlayA * clamp(overlayAmount, 0.0, 1.0);
    vec4 scrim = vec4(overlayR, overlayG, overlayB, scrimA);
    color = scrim + color * (1.0 - scrim.a);
    fragColor = color * mask * qt_Opacity;
}
