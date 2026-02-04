# -*- coding: utf-8 -*-
"""
GeoEngine Tools - ArcGIS Pro Python Toolbox
Connects to the GeoEngine proxy service to execute containerized geoprocessing tools.
"""

import arcpy
import os
import json
import time
from geoengine_client import GeoEngineClient


class Toolbox:
    def __init__(self):
        """Define the toolbox (the name of the toolbox is the name of the .pyt file)."""
        self.label = "GeoEngine Tools"
        self.alias = "geoengine"

        # Discover tools from GeoEngine service
        self.tools = self._discover_tools()

    def _discover_tools(self):
        """Discover available tools from the GeoEngine service."""
        try:
            client = GeoEngineClient()
            projects = client.list_projects()

            tools = []
            for project in projects:
                project_tools = client.get_project_tools(project['name'])
                for tool_info in project_tools:
                    # Create a dynamic tool class for each tool
                    tool_class = self._create_tool_class(project['name'], tool_info)
                    tools.append(tool_class)

            return tools if tools else [GeoEngineStatusTool]
        except Exception as e:
            arcpy.AddWarning(f"Could not connect to GeoEngine service: {e}")
            return [GeoEngineStatusTool]

    def _create_tool_class(self, project_name, tool_info):
        """Create a dynamic tool class for a GeoEngine tool."""

        class DynamicTool:
            def __init__(self):
                self.label = tool_info.get('label', tool_info['name'])
                self.description = tool_info.get('description', '')
                self.category = project_name
                self.canRunInBackground = True
                self._project = project_name
                self._tool_name = tool_info['name']
                self._inputs = tool_info.get('inputs', [])
                self._outputs = tool_info.get('outputs', [])

            def getParameterInfo(self):
                """Define parameter definitions."""
                params = []

                # Input parameters
                for i, inp in enumerate(self._inputs):
                    param = self._create_parameter(inp, 'Input')
                    params.append(param)

                # Output parameters
                for i, out in enumerate(self._outputs):
                    param = self._create_parameter(out, 'Output')
                    param.direction = 'Output'
                    params.append(param)

                return params

            def _create_parameter(self, param_info, direction):
                """Create an arcpy parameter from tool parameter info."""
                param_type = param_info.get('param_type', 'string')

                # Map GeoEngine types to ArcGIS types
                type_map = {
                    'raster': 'GPRasterLayer',
                    'vector': 'GPFeatureLayer',
                    'string': 'GPString',
                    'int': 'GPLong',
                    'float': 'GPDouble',
                    'bool': 'GPBoolean',
                    'folder': 'DEFolder',
                    'file': 'DEFile',
                }

                arcpy_type = type_map.get(param_type, 'GPString')
                required = param_info.get('required', True)

                param = arcpy.Parameter(
                    displayName=param_info.get('label', param_info['name']),
                    name=param_info['name'],
                    datatype=arcpy_type,
                    parameterType='Required' if required else 'Optional',
                    direction=direction,
                )

                # Set default value if provided
                if 'default' in param_info and param_info['default'] is not None:
                    param.value = param_info['default']

                return param

            def isLicensed(self):
                return True

            def updateParameters(self, parameters):
                return

            def updateMessages(self, parameters):
                return

            def execute(self, parameters, messages):
                """Execute the tool."""
                try:
                    client = GeoEngineClient()

                    # Build inputs dict
                    inputs = {}
                    output_dir = None

                    for param in parameters:
                        if param.direction == 'Input' and param.value is not None:
                            # Convert to string path if it's a dataset
                            if hasattr(param.value, 'dataSource'):
                                inputs[param.name] = param.value.dataSource
                            else:
                                inputs[param.name] = str(param.value)
                        elif param.direction == 'Output':
                            # Use output parameter location as output directory
                            if param.value:
                                output_path = str(param.value)
                                output_dir = os.path.dirname(output_path)

                    # Submit job
                    messages.addMessage(f"Submitting job to GeoEngine...")
                    job_id = client.submit_job(
                        project=self._project,
                        tool=self._tool_name,
                        inputs=inputs,
                        output_dir=output_dir
                    )

                    messages.addMessage(f"Job submitted: {job_id}")

                    # Poll for completion
                    while True:
                        status = client.get_job_status(job_id)

                        if status['status'] == 'completed':
                            messages.addMessage("Job completed successfully!")
                            break
                        elif status['status'] == 'failed':
                            messages.addErrorMessage(f"Job failed: {status.get('error', 'Unknown error')}")
                            return
                        elif status['status'] == 'cancelled':
                            messages.addWarningMessage("Job was cancelled")
                            return

                        # Update progress
                        messages.addMessage(f"Status: {status['status']}")
                        time.sleep(5)

                    # Get outputs
                    outputs = client.get_job_output(job_id)
                    messages.addMessage(f"Output files: {outputs}")

                except Exception as e:
                    messages.addErrorMessage(f"Error executing tool: {e}")

            def postExecute(self, parameters):
                return

        # Set a unique class name
        DynamicTool.__name__ = f"{project_name}_{tool_info['name']}"
        return DynamicTool


class GeoEngineStatusTool:
    """Tool to check GeoEngine service status."""

    def __init__(self):
        self.label = "Check GeoEngine Status"
        self.description = "Check the connection to the GeoEngine proxy service"
        self.canRunInBackground = False

    def getParameterInfo(self):
        port_param = arcpy.Parameter(
            displayName="Service Port",
            name="port",
            datatype="GPLong",
            parameterType="Optional",
            direction="Input"
        )
        port_param.value = 9876
        return [port_param]

    def isLicensed(self):
        return True

    def updateParameters(self, parameters):
        return

    def updateMessages(self, parameters):
        return

    def execute(self, parameters, messages):
        port = parameters[0].value or 9876

        try:
            client = GeoEngineClient(port=port)
            health = client.health_check()

            messages.addMessage(f"GeoEngine Service Status: {health['status']}")
            messages.addMessage(f"Version: {health['version']}")

            # List projects
            projects = client.list_projects()
            if projects:
                messages.addMessage(f"\nRegistered Projects ({len(projects)}):")
                for p in projects:
                    messages.addMessage(f"  - {p['name']} ({p['tools_count']} tools)")
            else:
                messages.addMessage("\nNo projects registered.")

        except Exception as e:
            messages.addErrorMessage(f"Cannot connect to GeoEngine service: {e}")
            messages.addMessage("\nMake sure the service is running:")
            messages.addMessage("  geoengine service start")

    def postExecute(self, parameters):
        return
