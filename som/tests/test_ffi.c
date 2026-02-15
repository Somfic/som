#include <stdio.h>
#include <stdint.h>

typedef struct
{
    uint32_t x;
    uint32_t y;
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
Vec2 make_vec2(uint32_t x, uint32_t y)
{
    return (Vec2){x, y};
}

// Accept struct by value
void print_vec2(Vec2 v)
{
    printf("Vec2(%u, %u)\n", v.x, v.y);
}

// Assert Vec2 values, returns 1 if correct, 0 if wrong
int32_t assert_vec2(Vec2 v, uint32_t expected_x, uint32_t expected_y)
{
    if (v.x == expected_x && v.y == expected_y)
    {
        return 1;
    }
    printf("assert_vec2 failed: expected (%u, %u), got (%u, %u)\n",
           expected_x, expected_y, v.x, v.y);
    return 0;
}

// Accept struct by value, return struct by value
Vec2 add_vec2(Vec2 a, Vec2 b)
{
    return (Vec2){a.x + b.x, a.y + b.y};
}

// Small struct (fits in one register on most ABIs)
void print_color(Color c)
{
    printf("Color(%u, %u, %u, %u)\n", c.r, c.g, c.b, c.a);
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
