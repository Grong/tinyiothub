# tiny iot hub

这是一个小型的物联网平台，用于将各类设备统一连接到平台进行管理，给 AI 提供底座支持。

## 技术栈

- 后端 rust loco
- 前端 react

## 设备模型定义

```json

{
  "identifier": "dtmi:com:example:Thermostat;1",
  "name": "智能温控器",
  "description": "温控器设备模型",
  "version": "1.0.2",
  "schemaVersion": "1.0",
  "createdAt": "2023-10-10T08:00:00Z",
  "lastUpdated": "2023-10-15T14:30:00Z",
  
  "extensions": {
    "manufacturer": "XXX科技",
    "deviceType": "climate-control",
    "protocol": "MQTT 3.1.1",
    "category": "智能家居"
  },
  
  "properties": [
    {
      "identifier": "power_status",
      "name": "电源状态",
      "description": "设备电源开关状态",
      "dataType": "bool",
      "accessMode": "rw",
      "dataSpecs": {
        "trueValue": "开启",
        "falseValue": "关闭"
      }
    },
    {
      "identifier": "humidity",
      "name": "环境湿度",
      "description": "当前环境湿度百分比",
      "dataType": "double",
      "accessMode": "r",
      "dataSpecs": {
        "min": 0,
        "max": 100,
        "step": 0.1,
        "unit": "%"
      }
    },
    {
      "identifier": "current_temperature",
      "name": "当前温度",
      "description": "传感器检测到的当前温度",
      "dataType": "double",
      "accessMode": "r",
      "dataSpecs": {
        "min": -20,
        "max": 50,
        "step": 0.1,
        "unit": "°C"
      }
    },
    {
      "identifier": "target_temperature",
      "name": "目标温度",
      "description": "用户设定的目标温度",
      "dataType": "double",
      "accessMode": "rw",
      "dataSpecs": {
        "min": 16,
        "max": 30,
        "step": 0.5,
        "unit": "°C"
      }
    },
    {
      "identifier": "operation_mode",
      "name": "运行模式",
      "description": "设备当前运行模式",
      "dataType": "enum",
      "accessMode": "rw",
      "dataSpecs": {
        "0": "自动",
        "1": "制冷",
        "2": "制热",
        "3": "送风",
        "4": "除湿"
      }
    },
    {
      "identifier": "fan_speed",
      "name": "风速",
      "description": "风扇运行速度",
      "dataType": "enum",
      "accessMode": "rw",
      "dataSpecs": {
        "0": "自动",
        "1": "低速",
        "2": "中速",
        "3": "高速"
      }
    },
    {
      "identifier": "battery_level",
      "name": "电池电量",
      "description": "设备剩余电池电量",
      "dataType": "int",
      "accessMode": "r",
      "dataSpecs": {
        "min": 0,
        "max": 100,
        "unit": "%"
      }
    }
  ],
  
  "services": [
    {
      "identifier": "reboot",
      "name": "重启设备",
      "description": "软重启设备",
      "callType": "async",
      "inputParams": []
    },
    {
      "identifier": "set_temperature",
      "name": "设置目标温度",
      "description": "设置期望达到的目标温度",
      "callType": "async",
      "inputParams": [
        {
          "identifier": "target_temp",
          "name": "目标温度值",
          "dataType": "double",
          "dataSpecs": {
            "min": 16,
            "max": 30,
            "step": 0.5,
            "unit": "°C"
          }
        },
        {
          "identifier": "delay_minutes",
          "name": "延迟时间",
          "dataType": "int",
          "required": false,
          "dataSpecs": {
            "min": 0,
            "max": 120,
            "unit": "分钟"
          }
        }
      ]
    },
    {
      "identifier": "reset_settings",
      "name": "恢复出厂设置",
      "description": "将所有设置恢复为出厂默认值",
      "callType": "async",
      "inputParams": [
        {
          "identifier": "confirm",
          "name": "确认操作",
          "dataType": "bool",
          "description": "设置为true确认执行重置"
        }
      ]
    }
  ],
  
  "events": [
    {
      "identifier": "over_temperature",
      "name": "温度过高告警",
      "description": "当温度超过安全阈值时触发",
      "eventType": "alert",
      "severity": "high",
      "outputData": [
        {
          "identifier": "current_temp",
          "name": "当前温度",
          "dataType": "float",
          "dataSpecs": {
            "unit": "°C"
          }
        },
        {
          "identifier": "threshold",
          "name": "告警阈值",
          "dataType": "float",
          "dataSpecs": {
            "unit": "°C"
          }
        },
        {
          "identifier": "timestamp",
          "name": "事件时间",
          "dataType": "timestamp"
        }
      ]
    },
    {
      "identifier": "low_battery",
      "name": "低电量告警",
      "description": "当电池电量低于20%时触发",
      "eventType": "warning",
      "severity": "medium",
      "outputData": [
        {
          "identifier": "battery_level",
          "name": "剩余电量",
          "dataType": "int",
          "dataSpecs": {
            "unit": "%"
          }
        },
        {
          "identifier": "timestamp",
          "name": "事件时间",
          "dataType": "timestamp"
        }
      ]
    },
    {
      "identifier": "device_started",
      "name": "设备启动完成",
      "description": "设备完成启动过程时触发",
      "eventType": "info",
      "severity": "low",
      "outputData": [
        {
          "identifier": "firmware_version",
          "name": "固件版本",
          "dataType": "string"
        },
        {
          "identifier": "boot_time",
          "name": "启动耗时",
          "dataType": "int",
          "dataSpecs": {
            "unit": "秒"
          }
        },
        {
          "identifier": "timestamp",
          "name": "事件时间",
          "dataType": "timestamp"
        }
      ]
    }
  ],
  
  "modules": [
    {
      "name": "temperature_control",
      "description": "温度控制模块",
      "properties": [
        "current_temperature", 
        "target_temperature", 
        "operation_mode"
      ],
      "services": [
        "set_temperature"
      ],
      "events": [
        "over_temperature"
      ]
    },
    {
      "name": "power_management",
      "description": "电源管理模块",
      "properties": [
        "power_status", 
        "battery_level"
      ],
      "services": [
        "reboot"
      ],
      "events": [
        "low_battery"
      ]
    }
  ]
}
```


## loco 脚手架

curd

``` 
cargo loco generate scaffold 模型名称 title:string content:text --api
cargo loco generate scaffold tag_bindings tenant_id:string tag_id:int target_id:int created_by:int --api

```