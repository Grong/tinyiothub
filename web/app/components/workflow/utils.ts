// Workflow utility functions
// This is a placeholder file to resolve import errors

export const workflowUtils = {
  // Add workflow utility functions here as needed
  placeholder: true,
}

// Keyboard utility functions
export const getKeyboardKeyCodeBySystem = () => {
  // TODO: Implement keyboard key code detection based on system
  const isMac = typeof navigator !== 'undefined' && navigator.platform.toUpperCase().indexOf('MAC') >= 0
  return isMac ? 'Cmd' : 'Ctrl'
}

export const getKeyboardKeyNameBySystem = () => {
  // TODO: Implement keyboard key name detection based on system
  const isMac = typeof navigator !== 'undefined' && navigator.platform.toUpperCase().indexOf('MAC') >= 0
  return isMac ? 'Command' : 'Control'
}

export default workflowUtils