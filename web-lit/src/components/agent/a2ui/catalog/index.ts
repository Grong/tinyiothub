/**
 * A2UI Catalog Registry
 * Maps component type strings to Lit component classes
 */

// Basic Catalog
import './basic/a2ui-text'
import './basic/a2ui-row'
import './basic/a2ui-column'
import './basic/a2ui-card'
import './basic/a2ui-button'
import './basic/a2ui-divider'

// IoT Catalog
import './device-card'
import './device-table'
import './data-chart'
import './control-panel'
import './confirmation-dialog'
import './progress-indicator'
import './real-time-toggle'

const registry = new Map<string, string>()

// Basic Catalog (type → tag name)
registry.set('Text', 'a2ui-text')
registry.set('Row', 'a2ui-row')
registry.set('Column', 'a2ui-column')
registry.set('Card', 'a2ui-card')
registry.set('Button', 'a2ui-button')
registry.set('Divider', 'a2ui-divider')

// IoT Catalog
registry.set('DeviceCard', 'device-card')
registry.set('DeviceTable', 'device-table')
registry.set('DataChart', 'data-chart')
registry.set('ControlPanel', 'control-panel')
registry.set('ConfirmationDialog', 'confirmation-dialog')
registry.set('ProgressIndicator', 'progress-indicator')
registry.set('RealTimeToggle', 'real-time-toggle')

export function getTagName(type: string): string | undefined {
  return registry.get(type)
}

export function getRegisteredTypes(): string[] {
  return Array.from(registry.keys())
}
