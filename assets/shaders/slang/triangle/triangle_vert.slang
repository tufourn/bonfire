cbuffer Uniforms
{
    float4x4 modelViewProjection;
}

struct CoarseVertex
{
    float3 color;
};

struct VertexStageOutput
{
    CoarseVertex    coarseVertex    : CoarseVertex;
    float4          sv_position     : SV_Position;
};

static const float3 vertexPositions[3] = {
    float3( 0.0,  0.5, 0.0),
    float3(-0.5, -0.5, 0.0),
    float3( 0.5, -0.5, 0.0)
};

static const float3 vertexColors[3] = {
    float3(1.0, 0.0, 0.0),
    float3(0.0, 1.0, 0.0),
    float3(0.0, 0.0, 1.0)
};

[shader("vertex")]
VertexStageOutput main(uint vertexID : SV_VertexID)
{
    VertexStageOutput output;

    float3 position = vertexPositions[vertexID];
    float3 color    = vertexColors[vertexID];

    output.coarseVertex.color = color;
    output.sv_position = float4(position, 1.0);

    return output;
}
