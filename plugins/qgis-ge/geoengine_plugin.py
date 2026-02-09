# -*- coding: utf-8 -*-
"""
GeoEngine QGIS Plugin - Main plugin class
"""

from qgis.core import QgsApplication
from qgis.PyQt.QtWidgets import QAction, QMessageBox
from qgis.PyQt.QtGui import QIcon
import os

from .geoengine_provider import GeoEngineProvider, GeoEngineCLIClient


class GeoEnginePlugin:
    """QGIS Plugin Implementation for GeoEngine."""

    def __init__(self, iface):
        """Constructor.

        Args:
            iface: An interface instance that will be passed to this class
                which provides the hook by which you can manipulate the QGIS
                application at run time.
        """
        self.iface = iface
        self.provider = None
        self.actions = []
        self.menu = '&GeoEngine'

    def initProcessing(self):
        """Initialize the processing provider."""
        self.provider = GeoEngineProvider()
        QgsApplication.processingRegistry().addProvider(self.provider)

    def initGui(self):
        """Create the menu entries and toolbar icons inside the QGIS GUI."""
        # Initialize processing provider
        self.initProcessing()

        # Add menu action to check service status
        self.add_action(
            'geoengine_status',
            'Check GeoEngine Status',
            self.show_status,
            menu=self.menu,
        )

        # Add menu action to refresh tools
        self.add_action(
            'geoengine_refresh',
            'Refresh Tools',
            self.refresh_tools,
            menu=self.menu,
        )

    def add_action(
        self,
        icon_name,
        text,
        callback,
        enabled=True,
        menu=None,
        toolbar=None,
    ):
        """Add a toolbar icon and menu item."""
        action = QAction(text, self.iface.mainWindow())
        action.triggered.connect(callback)
        action.setEnabled(enabled)

        if menu:
            self.iface.addPluginToMenu(menu, action)

        if toolbar:
            toolbar.addAction(action)

        self.actions.append(action)
        return action

    def unload(self):
        """
        Unregister plugin actions from QGIS menus and remove the GeoEngine processing provider from the processing registry.
        
        This removes all actions previously added to the plugin menu and, if a provider is registered, unregisters it from QgsApplication.processingRegistry().
        """
        # Remove menu items
        for action in self.actions:
            self.iface.removePluginMenu(self.menu, action)

        # Remove provider
        if self.provider:
            QgsApplication.processingRegistry().removeProvider(self.provider)

    def show_status(self):
        """
        Display GeoEngine CLI status in an information dialog.
        
        Queries the local GeoEngine CLI for version and project list, then shows a dialog with GeoEngine version, status, number of registered projects, and each project's name with its tools count. If the geoengine CLI executable is not found, shows a warning advising installation and PATH setup; on other errors shows a warning with the encountered error message.
        """
        try:
            client = GeoEngineCLIClient()
            info = client.version_check()
            projects = client.list_projects()

            msg = f"GeoEngine: {info['version']}\n"
            msg += f"Status: {info['status']}\n\n"
            msg += f"Registered Projects: {len(projects)}\n"

            for p in projects:
                msg += f"  - {p['name']} ({p.get('tools_count', 0)} tools)\n"

            QMessageBox.information(
                self.iface.mainWindow(),
                "GeoEngine Status",
                msg
            )

        except FileNotFoundError as e:
            QMessageBox.warning(
                self.iface.mainWindow(),
                "GeoEngine Error",
                f"{e}\n\n"
                "Install geoengine and ensure it is on your PATH:\n"
                "  https://github.com/NikaGeospatial/geoengine"
            )
        except Exception as e:
            QMessageBox.warning(
                self.iface.mainWindow(),
                "GeoEngine Error",
                f"Error communicating with geoengine:\n{e}"
            )

    def refresh_tools(self):
        """
        Refreshes the GeoEngine processing provider and updates the available tool list.
        
        If a provider is registered, removes it from the QGIS processing registry and replaces it with a newly initialized provider so the available algorithms/tools are reloaded. Shows an informational dialog on success and a warning dialog containing the error message on failure.
        """
        try:
            if self.provider:
                # Remove and re-add provider to refresh algorithms
                QgsApplication.processingRegistry().removeProvider(self.provider)
                self.provider = GeoEngineProvider()
                QgsApplication.processingRegistry().addProvider(self.provider)

                QMessageBox.information(
                    self.iface.mainWindow(),
                    "GeoEngine",
                    "Tools refreshed successfully!"
                )
        except Exception as e:
            QMessageBox.warning(
                self.iface.mainWindow(),
                "GeoEngine Error",
                f"Failed to refresh tools:\n{e}"
            )