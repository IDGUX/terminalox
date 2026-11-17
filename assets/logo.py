import math, sys
from PIL import Image, ImageDraw, ImageFilter

S = 4096                      # supersampled canvas
OUT = sys.argv[1]
img = Image.new("RGBA", (S, S), (0, 0, 0, 0))

# --- background squircle (macOS grid: 824/1024 content, radius ~185/1024) ---
m = int(S * 100 / 1024)
r = int(S * 185 / 1024)
bg = Image.new("RGBA", (S, S))
top, bot = (30, 41, 59), (9, 13, 24)
gd = ImageDraw.Draw(bg)
for y in range(S):
    t = y / S
    gd.line([(0, y), (S, y)], fill=tuple(int(top[i] + (bot[i] - top[i]) * t) for i in range(3)) + (255,))
mask = Image.new("L", (S, S), 0)
ImageDraw.Draw(mask).rounded_rectangle([m, m, S - m, S - m], r, fill=255)
img.paste(bg, (0, 0), mask)

d = ImageDraw.Draw(img)
# subtle top highlight border
d.rounded_rectangle([m, m, S - m, S - m], r, outline=(255, 255, 255, 28), width=int(S * 0.006))

# --- shield ---
cx = S / 2
w = S * 0.50          # shield width
top_y = S * 0.255
mid_y = S * 0.50
bot_y = S * 0.775
def shield_points(scale=1.0):
    hw = w / 2 * scale
    ty = cx + (top_y - cx) * scale
    my = cx + (mid_y - cx) * scale
    by = cx + (bot_y - cx) * scale
    pts = []
    # top edge with gentle dip in the middle
    for i in range(41):
        t = i / 40
        x = cx - hw + 2 * hw * t
        y = ty - math.sin(t * math.pi) * S * 0.012 * scale
        pts.append((x, y))
    # right side down then curve to bottom point (quadratic)
    p0, p1, p2 = (cx + hw, my), (cx + hw, by - (by - my) * 0.25), (cx, by)
    
    for i in range(41):
        t = i / 40
        x = (1-t)**2*p0[0] + 2*(1-t)*t*p1[0] + t*t*p2[0]
        y = (1-t)**2*p0[1] + 2*(1-t)*t*p1[1] + t*t*p2[1]
        pts.append((x, y))
    p0, p1, p2 = (cx, by), (cx - hw, by - (by - my) * 0.25), (cx - hw, my)
    for i in range(41):
        t = i / 40
        x = (1-t)**2*p0[0] + 2*(1-t)*t*p1[0] + t*t*p2[0]
        y = (1-t)**2*p0[1] + 2*(1-t)*t*p1[1] + t*t*p2[1]
        pts.append((x, y))
    
    return pts

# glow
glow = Image.new("RGBA", (S, S), (0, 0, 0, 0))
ImageDraw.Draw(glow).polygon(shield_points(1.04), fill=(52, 211, 153, 70))
glow = glow.filter(ImageFilter.GaussianBlur(S * 0.03))
img.alpha_composite(glow)

# shield fill (green gradient) and inner dark panel
fill = Image.new("RGBA", (S, S))
fd = ImageDraw.Draw(fill)
c1, c2 = (110, 231, 183), (16, 185, 129)
for y in range(S):
    t = min(max((y - top_y) / (bot_y - top_y), 0), 1)
    fd.line([(0, y), (S, y)], fill=tuple(int(c1[i] + (c2[i] - c1[i]) * t) for i in range(3)) + (255,))
smask = Image.new("L", (S, S), 0)
ImageDraw.Draw(smask).polygon(shield_points(1.0), fill=255)
img.paste(fill, (0, 0), smask)

inner = Image.new("L", (S, S), 0)
ImageDraw.Draw(inner).polygon(shield_points(0.80), fill=255)
panel = Image.new("RGBA", (S, S), (13, 18, 30, 255))
img.paste(panel, (0, 0), inner)

# --- prompt ">_" ---
lw = int(S * 0.048)
col = (229, 231, 235, 255)
def line(a, b):
    d.line([a, b], fill=col, width=lw)
    for p in (a, b):
        d.ellipse([p[0]-lw/2, p[1]-lw/2, p[0]+lw/2, p[1]+lw/2], fill=col)
ox, oy = cx - S*0.085, S*0.47
h = S*0.085
line((ox, oy - h), (ox + h*0.95, oy))
line((ox + h*0.95, oy), (ox, oy + h))
green = (110, 231, 183, 255)
ux0, ux1, uy = cx + S*0.03, cx + S*0.13, oy + h
d.line([(ux0, uy), (ux1, uy)], fill=green, width=lw)
for p in ((ux0, uy), (ux1, uy)):
    d.ellipse([p[0]-lw/2, p[1]-lw/2, p[0]+lw/2, p[1]+lw/2], fill=green)

img.resize((1024, 1024), Image.LANCZOS).save(OUT)
