#!/usr/bin/env python3
"""
BACnet 驱动集成测试脚本
"""

import json
import requests
import time
from typing import Dict, Any, Optional

# API 配置
API_BASE_URL = "http://localhost:8080/api/v1"
USERNAME = "admin"
PASSWORD = "admin123"

class BacnetDriverTester:
    def __init__(self):
        self.token: Optional[str] = None
        self.driver_id: Optional[int] = None
        self.device_id: Optional[int] = None
        
    def login(self) -> bool:
        """登录获取 token"""
        print("🔐 登录系统...")
        response = requests.post(
            f"{API_BASE_URL}/auth/login",
            json={"username": USERNAME, "password": PASSWORD}
        )
        
        if response.status_code == 200:
            data = response.json()
            if data.get("code") == 0:
                self.token = data["result"]["token"]
                print(f"✅ 登录成功，token: {self.token[:20]}...")
                return True
        
        print(f"❌ 登录失败: {response.text}")
        return False
    
    def get_headers(self) -> Dict[str, str]:
        """获取请求头"""
        return {
            "Authorization": f"Bearer {self.token}",
            "Content-Type": "application/json"
        }
    
    def upload_driver(self) -> bool:
        """上传 BACnet 驱动"""
        print("\n📦 上传 BACnet 驱动...")
        
        # 构建驱动文件路径
        import platform
        system = platform.system()
        
        if system == "Windows":
            driver_path = "target/release/bacnet_driver.dll"
        elif system == "Darwin":
            driver_path = "target/release/libbacnet_driver.dylib"
        else:
            driver_path = "target/release/libbacnet_driver.so"
        
        try:
            with open(driver_path, "rb") as f:
                files = {"file": ("bacnet_driver", f, "application/octet-stream")}
                response = requests.post(
                    f"{API_BASE_URL}/drivers/upload",
                    headers={"Authorization": f"Bearer {self.token}"},
                    files=files
                )
                
                if response.status_code == 200:
                    data = response.json()
                    if data.get("code") == 0:
                        self.driver_id = data["result"]["id"]
                        print(f"✅ 驱动上传成功，ID: {self.driver_id}")
                        print(f"   名称: {data['result']['name']}")
                        print(f"   版本: {data['result']['version']}")
                        return True
                
                print(f"❌ 驱动上传失败: {response.text}")
                return False
                
        except FileNotFoundError:
            print(f"❌ 驱动文件不存在: {driver_path}")
            print("   请先编译驱动: cargo build --release")
            return False
    
    def list_drivers(self) -> bool:
        """列出所有驱动"""
        print("\n📋 查询驱动列表...")
        response = requests.get(
            f"{API_BASE_URL}/drivers",
            headers=self.get_headers()
        )
        
        if response.status_code == 200:
            data = response.json()
            if data.get("code") == 0:
                drivers = data["result"]
                print(f"✅ 找到 {len(drivers)} 个驱动:")
                for driver in drivers:
                    print(f"   - {driver['name']} v{driver['version']} (ID: {driver['id']})")
                    if driver['name'] == 'BacnetDriver':
                        self.driver_id = driver['id']
                return True
        
        print(f"❌ 查询驱动失败: {response.text}")
        return False
    
    def create_device(self) -> bool:
        """创建 BACnet 设备"""
        print("\n🔧 创建 BACnet 设备...")
        
        # BACnet 设备配置
        bacnet_config = {
            "device_instance": 1001,
            "ip_address": "192.168.1.100",
            "port": 47808,
            "object_mappings": [
                {
                    "name": "temperature",
                    "object_type": "analog-input",
                    "object_instance": 1,
                    "property": "present-value"
                },
                {
                    "name": "humidity",
                    "object_type": "analog-input",
                    "object_instance": 2,
                    "property": "present-value"
                },
                {
                    "name": "fan_status",
                    "object_type": "binary-value",
                    "object_instance": 10,
                    "property": "present-value"
                },
                {
                    "name": "mode",
                    "object_type": "multi-state-value",
                    "object_instance": 20,
                    "property": "present-value"
                }
            ]
        }
        
        device_data = {
            "name": "BACnet HVAC Controller",
            "driver_id": self.driver_id,
            "config": json.dumps(bacnet_config),
            "description": "BACnet building automation controller",
            "enabled": True
        }
        
        response = requests.post(
            f"{API_BASE_URL}/devices",
            headers=self.get_headers(),
            json=device_data
        )
        
        if response.status_code == 200:
            data = response.json()
            if data.get("code") == 0:
                self.device_id = data["result"]["id"]
                print(f"✅ 设备创建成功，ID: {self.device_id}")
                print(f"   名称: {data['result']['name']}")
                print(f"   驱动: {data['result']['driver_name']}")
                return True
        
        print(f"❌ 设备创建失败: {response.text}")
        return False
    
    def read_device_data(self) -> bool:
        """读取设备数据"""
        print("\n📊 读取设备数据...")
        response = requests.get(
            f"{API_BASE_URL}/devices/{self.device_id}/data",
            headers=self.get_headers()
        )
        
        if response.status_code == 200:
            data = response.json()
            if data.get("code") == 0:
                values = data["result"]
                print(f"✅ 读取到 {len(values)} 个数据点:")
                for value in values:
                    print(f"   - {value['name']}: {value['value']} ({value['type']})")
                return True
        
        print(f"❌ 读取数据失败: {response.text}")
        return False
    
    def execute_command(self) -> bool:
        """执行设备命令"""
        print("\n⚡ 执行设备命令...")
        
        command_data = {
            "name": "fan_status",
            "params": {
                "value": "true"
            }
        }
        
        response = requests.post(
            f"{API_BASE_URL}/devices/{self.device_id}/command",
            headers=self.get_headers(),
            json=command_data
        )
        
        if response.status_code == 200:
            data = response.json()
            if data.get("code") == 0:
                print(f"✅ 命令执行成功")
                return True
        
        print(f"❌ 命令执行失败: {response.text}")
        return False
    
    def cleanup(self) -> bool:
        """清理测试数据"""
        print("\n🧹 清理测试数据...")
        
        success = True
        
        # 删除设备
        if self.device_id:
            response = requests.delete(
                f"{API_BASE_URL}/devices/{self.device_id}",
                headers=self.get_headers()
            )
            if response.status_code == 200:
                print(f"✅ 设备已删除 (ID: {self.device_id})")
            else:
                print(f"⚠️  删除设备失败: {response.text}")
                success = False
        
        # 删除驱动
        if self.driver_id:
            response = requests.delete(
                f"{API_BASE_URL}/drivers/{self.driver_id}",
                headers=self.get_headers()
            )
            if response.status_code == 200:
                print(f"✅ 驱动已删除 (ID: {self.driver_id})")
            else:
                print(f"⚠️  删除驱动失败: {response.text}")
                success = False
        
        return success
    
    def run_tests(self):
        """运行完整测试流程"""
        print("=" * 60)
        print("BACnet 驱动集成测试")
        print("=" * 60)
        
        try:
            # 1. 登录
            if not self.login():
                return False
            
            # 2. 上传驱动
            if not self.upload_driver():
                # 如果上传失败，尝试查询已存在的驱动
                if not self.list_drivers():
                    return False
                if not self.driver_id:
                    print("❌ 未找到 BACnet 驱动")
                    return False
            
            # 3. 创建设备
            if not self.create_device():
                return False
            
            # 等待设备初始化
            print("\n⏳ 等待设备初始化...")
            time.sleep(2)
            
            # 4. 读取数据
            if not self.read_device_data():
                return False
            
            # 5. 执行命令
            if not self.execute_command():
                return False
            
            # 6. 再次读取数据验证
            time.sleep(1)
            if not self.read_device_data():
                return False
            
            print("\n" + "=" * 60)
            print("✅ 所有测试通过！")
            print("=" * 60)
            
            return True
            
        except Exception as e:
            print(f"\n❌ 测试过程中发生错误: {e}")
            import traceback
            traceback.print_exc()
            return False
        
        finally:
            # 清理测试数据
            input("\n按 Enter 键清理测试数据...")
            self.cleanup()

def main():
    tester = BacnetDriverTester()
    success = tester.run_tests()
    exit(0 if success else 1)

if __name__ == "__main__":
    main()
