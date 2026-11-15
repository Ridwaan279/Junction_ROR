"""ROS camera/odometry -> existing Rust backend. No real aircraft integration.

    python3 -m emberwatch_drone.recon_bridge --api http://127.0.0.1:8081

Run ONLY with the isolated recon world/ROS domain from run_recon_demo.sh.
Network work and JPEG encoding stay off the 20 Hz control callback.
"""
from __future__ import annotations

import argparse
from collections import deque
from datetime import datetime, timezone
import io
import json
import signal
import threading
import time
import urllib.parse
import urllib.request

from PIL import Image as PILImage
import rclpy
from rclpy.node import Node
from rclpy.executors import ExternalShutdownException
from rclpy.qos import qos_profile_sensor_data
from geometry_msgs.msg import Twist
from nav_msgs.msg import Odometry
from sensor_msgs.msg import Image
from std_msgs.msg import String

from .recon import ReconFlight


def utc():
    return datetime.now(timezone.utc).isoformat()


def stamp(msg):
    return msg.header.stamp.sec + msg.header.stamp.nanosec * 1e-9


class ReconBridge(Node):
    def __init__(self, args):
        super().__init__("emberwatch_recon_bridge")
        self.args = args
        self.flight = ReconFlight(args.observe_s)
        self.pos, self.last_odom = None, 0.
        self.positions = deque(maxlen=120)
        self.camera = None
        self.task = None
        self.recall = False
        self.sequence = 0
        self.finished_sent = threading.Event()
        self.shutdown_worker = threading.Event()
        self.lock = threading.Lock()
        self.cmd = self.create_publisher(Twist, "/emberwatch/drone/cmd_vel", 10)
        self.status = self.create_publisher(String, "/emberwatch/recon/status", 10)
        self.create_subscription(Odometry, "/emberwatch/drone/odom", self.on_odom, 10)
        if not args.no_camera:
            self.create_subscription(Image, "/emberwatch/recon/camera", self.on_image, qos_profile_sensor_data)
        self.create_timer(.05, self.tick)
        self.worker = threading.Thread(target=self.network, daemon=True)
        self.worker.start()

    def on_odom(self, msg):
        p = msg.pose.pose.position
        with self.lock:
            self.pos, self.last_odom = (p.x, p.y, p.z), time.monotonic()
            self.positions.append((stamp(msg), self.pos))

    def on_image(self, msg):
        with self.lock:
            nearest = min(self.positions, key=lambda p: abs(p[0] - stamp(msg)), default=None)
            position = nearest[1] if nearest and abs(nearest[0] - stamp(msg)) <= .2 else None
            self.camera = (msg, utc(), time.monotonic(), position)

    def tick(self):
        with self.lock:
            task, pos = self.task, self.pos
            if task and self.flight.state == "assigned" and pos:
                self.flight.assign(task, pos, time.monotonic(), self.args.force_no_route)
            velocity = self.flight.step(pos, time.monotonic(), time.monotonic() - self.last_odom <= 1., self.recall)
        msg = Twist()
        msg.linear.x, msg.linear.y, msg.linear.z = velocity
        self.cmd.publish(msg)

    def request(self, path, data=None, content_type="application/json"):
        request = urllib.request.Request(self.args.api.rstrip('/') + path, data=data,
                                         headers={"Content-Type": content_type})
        with urllib.request.urlopen(request, timeout=2) as response:
            return json.load(response)

    def network(self):
        query = "?run_id=" + urllib.parse.quote(self.args.run_id, safe="")
        last_image = None
        started = time.monotonic()
        last_success = started
        while not self.shutdown_worker.is_set():
            try:
                tasks = self.request("/api/recon/tasks" + query)["tasks"]
                with self.lock:
                    if self.task is None or self.flight.state in ("complete", "failed"):
                        candidate = next((t for t in tasks if t["drone_id"] == self.args.drone_id and t["observation"] is None), None)
                        if candidate:
                            self.task = candidate
                            self.flight = ReconFlight(self.args.observe_s)
                            self.finished_sent.clear()
                    task = self.task
                    current = next((t for t in tasks if task and t["id"] == task["id"]), None)
                    if task and not current:
                        # Seek/reset invalidates task; hold, don't keep flying an obsolete epoch.
                        self.flight.state, self.flight.note = "failed", "run_reset: task invalidated; holding zero velocity"
                        self.get_logger().error(self.flight.note)
                        return
                    self.recall = bool(current and current["requested_action"] == "return")
                    pos, camera = self.pos, self.camera
                    state, note = self.flight.state, self.flight.note
                    if task and pos is None and time.monotonic() - started > 15:
                        self.flight.state, self.flight.note = "failed", "no_telemetry: no simulator odometry"
                        state, note = self.flight.state, self.flight.note
                if task:
                    self.sequence += 1
                    fresh = camera is not None and time.monotonic() - camera[2] < 3.
                    jpeg = None
                    if fresh and camera[1] != last_image:
                        image, captured_at, _, image_pos = camera
                        if image.encoding not in ("rgb8", "bgr8", "rgba8", "bgra8"):
                            raise ValueError(f"unsupported camera encoding {image.encoding}")
                        mode = "RGBA" if "a8" in image.encoding else "RGB"
                        rawmode = {"rgb8":"RGB", "bgr8":"BGR", "rgba8":"RGBA", "bgra8":"BGRA"}[image.encoding]
                        im = PILImage.frombytes(mode, (image.width, image.height), bytes(image.data), "raw", rawmode, image.step).convert("RGB")
                        output = io.BytesIO()
                        im.save(output, format="JPEG", quality=80)
                        jpeg = output.getvalue()
                    body = {"sequence": self.sequence, "simulation": True, "state": state,
                            "observed_at": camera[1] if jpeg else utc(),
                            "sim_time_s": stamp(camera[0]) if jpeg else None,
                            "drone_position_enu_m": camera[3] if jpeg else pos,
                            "drone_position_lon_lat": None, "scene_is_geographic": False,
                            "camera_status": "available" if fresh else ("stale" if camera else "no_camera"),
                            "camera_topic": None if self.args.no_camera else "/emberwatch/recon/camera", "detection_method": "simulator_state", "note": note}
                    path = "/api/recon/tasks/" + task["id"]
                    self.request(path + "/observations" + query, json.dumps(body).encode())
                    if jpeg:
                        self.request(path + "/frame" + query + "&sequence=" + str(self.sequence), jpeg, "image/jpeg")
                        last_image = camera[1]
                    self.status.publish(String(data=json.dumps(body)))
                    self.get_logger().info(f"{state}: seq={self.sequence} camera={body['camera_status']} uploaded={jpeg is not None}")
                    if state in ("complete", "failed"):
                        self.finished_sent.set()
                last_success = time.monotonic()
            except Exception as exc:
                self.get_logger().warning(f"recon backend/camera unavailable: {type(exc).__name__}: {exc}")
                if self.task and time.monotonic() - last_success > 8:
                    with self.lock:
                        self.flight.state, self.flight.note = "failed", "backend_stale: holding zero velocity; operator link unavailable"
            self.shutdown_worker.wait(1.)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--api", default="http://127.0.0.1:8081")
    parser.add_argument("--run-id", default="synthetic-portugal")
    parser.add_argument("--drone-id", default="ember_drone")
    parser.add_argument("--observe-s", type=float, default=15.)
    parser.add_argument("--force-no-route", action="store_true")
    parser.add_argument("--no-camera", action="store_true", help="Exercise unavailable-camera handling; do not consume frames")
    parser.add_argument("--exit-when-done", action="store_true")
    args = parser.parse_args()
    if args.observe_s <= 0 or args.observe_s > 60:
        parser.error("--observe-s must be in (0, 60]")
    rclpy.init(args=[])
    def terminate(_signum, _frame):
        raise KeyboardInterrupt
    signal.signal(signal.SIGTERM, terminate)
    node = ReconBridge(args)
    try:
        while rclpy.ok() and not (args.exit_when_done and node.finished_sent.is_set()):
            rclpy.spin_once(node, timeout_sec=.1)
    except (KeyboardInterrupt, ExternalShutdownException):
        pass
    finally:
        if rclpy.ok():
            node.cmd.publish(Twist())
        node.shutdown_worker.set()
        node.worker.join(timeout=7)
        node.destroy_node()
        if rclpy.ok():
            rclpy.shutdown()


if __name__ == "__main__":
    main()
