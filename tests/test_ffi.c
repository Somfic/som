#include <stdio.h>
#include <stdint.h>

typedef struct
{
    float x;
    float y;
} Vec2;
typedef struct
{
    uint8_t r;
    uint8_t g;
    uint8_t b;
    uint8_t a;
} Color;

int32_t identity(int32_t x)
{
    return x;
}

// Return struct by value
Vec2 make_vec2(float x, float y)
{
    return (Vec2){x, y};
}

// Accept struct by value
void print_vec2(Vec2 v)
{
    printf("Vec2(%f, %f)\n", v.x, v.y);
}

// Accept struct by value, return struct by value
Vec2 add_vec2(Vec2 a, Vec2 b)
{
    return (Vec2){a.x + b.x, a.y + b.y};
}

// Small struct (fits in one register on most ABIs)
void print_color(Color c)
{
    printf("Color(%d, %d, %d, %d)\n", c.r, c.g, c.b, c.a);
}

// Bigger struct (likely passed on stack)
typedef struct
{
    float x;
    float y;
    float z;
    float w;
    float v;
} Big;
void print_big(Big b)
{
    printf("Big(%f, %f, %f, %f, %f)\n", b.x, b.y, b.z, b.w, b.v);
}
