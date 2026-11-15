from glob import glob

from setuptools import find_packages, setup

package_name = "emberwatch_drone"

setup(
    name=package_name,
    version="0.1.0",
    packages=find_packages(exclude=["test"]),
    data_files=[
        ("share/ament_index/resource_index/packages", ["resource/" + package_name]),
        ("share/" + package_name, ["package.xml"]),
        ("share/" + package_name + "/launch", glob("launch/*.py")),
        ("share/" + package_name + "/config", glob("config/*")),
        ("share/" + package_name + "/worlds", glob("worlds/*.sdf")),
        ("share/" + package_name + "/worlds/media", glob("worlds/media/*")),
    ],
    install_requires=["setuptools"],
    zip_safe=True,
    maintainer="EmberWatch team",
    maintainer_email="noreply@example.org",
    description="Simulated single-drone mission adapter for EmberWatch reports",
    license="MIT",
    tests_require=["pytest"],
    entry_points={
        "console_scripts": [
            "plan_mission = emberwatch_drone.cli:main",
            "kinematic_mission = emberwatch_drone.kinematic_mission_node:main",
            "px4_offboard_mission = emberwatch_drone.px4_offboard_node:main",
            "plan_scenario = emberwatch_drone.scenario_cli:main",
            "fire_sim = emberwatch_drone.fire_sim_node:main",
            "px4_fire_mission = emberwatch_drone.px4_fire_mission_node:main",
            "gen_arganil_world = emberwatch_drone.arganil.worldgen:main",
            "plan_arganil_scenario = emberwatch_drone.arganil.scenario:main",
        ],
    },
)
