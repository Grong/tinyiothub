// Placeholder for plugin service
// 插件服务占位文件（HarmonyOS 部署时不需要）

export const useInstalledPluginList = () => {
  return {
    data: {
      total: 0,
      plugins: [],
    },
    isLoading: false,
    error: null,
  }
}

export const usePluginReadmeAsset = (pluginInfo: any) => {
  return {
    data: null,
    isLoading: false,
    error: null,
  }
}
