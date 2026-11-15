import math

from emberwatch_drone.path_follow import PathFollower, YawLimiter


def test_carrot_stays_on_path_and_progresses():
    pts = [(0, 0, 10), (50, 0, 10), (50, 50, 10), (50, 50, 2)]
    f = PathFollower(pts, lookahead_m=8.0)
    pos = (0.0, 0.0, 10.0)
    for _ in range(400):  # drone moves 60% of the way to the carrot each tick
        sp, heading, rem = f.update(pos)
        # carrot is ON the polyline
        assert min(_seg_dist(sp, a, b) for a, b in zip(pts, pts[1:])) < 1e-6
        pos = tuple(p + 0.6 * (q - p) for p, q in zip(pos, sp))
    assert rem < 0.5 and math.dist(pos, pts[-1]) < 0.5


def test_corner_cut_bounded():
    pts = [(0, 0, 10), (40, 0, 10), (40, 40, 10)]
    f = PathFollower(pts, lookahead_m=3.5)
    pos, worst = (0.0, 0.0, 10.0), 0.0
    for _ in range(600):
        sp, _, _ = f.update(pos)
        pos = tuple(p + 0.3 * (q - p) for p, q in zip(pos, sp))
        worst = max(worst, min(_seg_dist(pos, a, b) for a, b in zip(pts, pts[1:])))
    assert worst < 3.5 * (1 - math.cos(math.radians(45))) + 0.5


def test_yaw_rate_limited_and_wraps():
    y = YawLimiter(math.radians(30))
    assert y.step(math.radians(170), 0.1) == math.radians(170)
    v = y.step(math.radians(-170), 0.1)  # 20 deg across the wrap, limited to 3 deg/tick
    assert abs(((v - math.radians(173)) + math.pi) % (2 * math.pi) - math.pi) < 1e-9


def _seg_dist(p, a, b):
    ab = [y - x for x, y in zip(a, b)]
    L2 = sum(v * v for v in ab)
    t = 0 if L2 == 0 else max(0, min(1, sum((pp - x) * v for pp, x, v in zip(p, a, ab)) / L2))
    return math.dist(p, [x + v * t for x, v in zip(a, ab)])
