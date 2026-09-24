#version 440

layout(location = 0) in vec4 qt_Vertex;
layout(location = 1) in vec2 qt_MultiTexCoord0;
layout(location = 0) out vec2 qt_TexCoord0;

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

void main()
{
    qt_TexCoord0 = qt_MultiTexCoord0;
    gl_Position = qt_Matrix * qt_Vertex;
}
