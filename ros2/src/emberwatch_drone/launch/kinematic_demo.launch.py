"""Gazebo scene + explicit ros_gz bridge + ONE simulated (kinematic) drone mission.

    ros2 launch emberwatch_drone kinematic_demo.launch.py snapshot_file:=/abs/path/snapshot.json
    ros2 launch emberwatch_drone kinematic_demo.launch.py api_base:=http://127.0.0.1:8080 run_id:=synthetic-portugal
    ... headless:=true   # server only (no GUI window)
"""

import os

from ament_index_python.packages import get_package_share_directory
from launch import LaunchDescription
from launch.actions import (DeclareLaunchArgument, EmitEvent, ExecuteProcess, RegisterEventHandler,
                            SetEnvironmentVariable)
from launch.conditions import IfCondition, UnlessCondition
from launch.event_handlers import OnProcessExit
from launch.events import Shutdown
from launch.substitutions import LaunchConfiguration as LC
from launch_ros.actions import Node


def generate_launch_description():
    share = get_package_share_directory("emberwatch_drone")
    world = os.path.join(share, "worlds", "emberwatch_scene.sdf")
    bridge_cfg = os.path.join(share, "config", "bridge.yaml")
    resource_path = os.pathsep.join(
        p for p in [os.path.join(share, "worlds"), os.environ.get("GZ_SIM_RESOURCE_PATH", "")] if p)

    mission = Node(
        package="emberwatch_drone",
        executable="kinematic_mission",
        output="screen",
        parameters=[{
            "snapshot_file": LC("snapshot_file"),
            "api_base": LC("api_base"),
            "run_id": LC("run_id"),
            "incident_id": LC("incident_id"),
        }],
    )
    headless = LC("headless")
    return LaunchDescription([
        DeclareLaunchArgument("snapshot_file", default_value=""),
        DeclareLaunchArgument("api_base", default_value=""),
        DeclareLaunchArgument("run_id", default_value="synthetic-portugal"),
        DeclareLaunchArgument("incident_id", default_value=""),
        DeclareLaunchArgument("headless", default_value="false"),
        DeclareLaunchArgument("exit_when_done", default_value="false"),
        SetEnvironmentVariable("GZ_SIM_RESOURCE_PATH", resource_path),
        ExecuteProcess(cmd=["gz", "sim", "-r", "-s", "--headless-rendering", world], output="screen",
                       condition=IfCondition(headless)),
        ExecuteProcess(cmd=["gz", "sim", "-r", world], output="screen",
                       condition=UnlessCondition(headless)),
        Node(package="ros_gz_bridge", executable="parameter_bridge", output="screen",
             parameters=[{"config_file": bridge_cfg}]),
        mission,
        RegisterEventHandler(
            OnProcessExit(target_action=mission, on_exit=[EmitEvent(event=Shutdown(reason="mission finished"))]),
            condition=IfCondition(LC("exit_when_done"))),
    ])
