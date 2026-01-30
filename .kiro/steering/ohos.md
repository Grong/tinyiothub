## 限定

- 这里的鸿蒙指的是OpenHarmony

## 设备

-  Linux localhost 5.10.184 #39 SMP Mon Sep 8 18:05:05 CST 2025 aarch64

## 指令

```
hdc file send [-a|-sync|-z|-m|-cwd path|-b bundlename] SOURCE DEST

参数名	说明
SOURCE	本地待传输的文件路径。
DEST	
远程目标文件路径。

从API version 21开始，媒体库文件支持通过hdc进行部分操作（低版本使用会提示[Fail]Error opening file: ...）。

媒体库文件路径：/mnt/data/<uid>/media_fuse/Photo/目录及其子目录，<uid>为当前用户的id。

通过hdc对媒体库操作指导参见mediatool。

-a	保留文件修改时间戳。
-sync	
只传输文件mtime有更新的文件。

mtime（modified timestamp）：修改后的时间戳。

-z	通过LZ4格式压缩传输，此功能未开放，请勿使用。
-m	
文件传输时同步文件DAC权限，uid，gid，MAC权限。

DAC（Discretionary Access Control）：自主访问控制，

uid（User identifier）：用户标识符（或用户ID），

gid（Group identifier）：组标识符（或组ID），

MAC（Mandatory Access Control）：强制访问控制（或非自主访问控制）。

-cwd	
修改工作目录。

用于在文件传输时，切换SOURCE到指定path。例如，初始发送文件为test，所在目录为/data，实际发送文件路径为/data/test；如果使用-cwd "/user/"，实际发送文件路径为/user/test。

-b	
3.1.0e版本新增参数（低版本使用会提示[Fail]Unknown file option: -b），用于指定可调试应用包名。

使用方法可参考通过命令往应用沙箱目录中发送文件。

bundlename	指定可调试应用包名。

```
- hdc 传输文件，不要重命名。
- hdc 传输文件，如果文件在子目录中，也会在设备中把子目录一起创建