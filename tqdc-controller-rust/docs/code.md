
Встроенный парсер [TqdcFileWriter::writeDecodedData](../afi-daq/apps/afi-tqdc2/src/afi-tqdc2/TqdcFileWriter.cpp?plain=1#L104)


[Старт набора](../afi-daq/apps/afi-tqdc2/src/afi-tqdc2/TQDCDevice.cpp?plain=1#L525)

[Отправка пакета mlink](../afi-daq/libs/mlinkip/src/regio/regiomlink.cpp?plain=1#L80)

[checksum](../afi-daq/libs/mlinkip/src/regio/regiomlink.cpp#L97)

[Десериализация пакета mlink](../afi-daq/libs/mlinkip/src/regio/regiomlink.cpp?plain=1#L122)

[регистры](../afi-daq/apps/afi-tqdc2/src/afi-tqdc2/TqdcRegs.h?plain=1#L26)

[запись 64-битного регистра](../afi-daq/libs/mlinkip/src/regio/mlinkdevice.cpp#L109)

[REG_DEVICE_ID](../afi-daq/libs/mlinkip/src/mregdev/mregdevice.h#L44)

[типы фреймов](../afi-daq/libs/mlinkip/src/mlink/frame_types.h)

[запись непосредственно в регистр](../afi-daq/libs/mlinkip/src/regio/mlinkdevice.cpp#L191)

---
connectToHardware (нужно?)
/home/chernov/projects/afi-daq/libs/mstream-lib/mstream-lib/MlinkStreamReceiver.cpp:290

[ответ на получение данных](../afi-daq/libs/mstream-lib/mstream-lib/MlinkStreamReceiver.cpp#L239)

[Обработка сообщения mstream](../afi-daq/libs/mstream-lib/mstream-lib/MStreamDump.cpp#L239)


MLinkFrameInfo
/home/chernov/projects/afi-daq/libs/mstream-lib/mstream-lib/types.h:87


### Настройка сети

- выставить адаптер в режим link-local only

- установка DHCP сервера
  ```bash
  sudo apt install isc-dhcp-server
  ```
  
  ```sh
  #/etc/dhcp/dhcpd.conf
  default-lease-time 600;
  max-lease-time 7200;

  subnet 10.0.0.0 netmask 255.255.255.0 {
    interface enp1s0;
    range 10.0.0.5 10.0.0.15;
  }
  ```
- настройка сети
  ```
  sudo ip addr add 10.0.0.4/24 dev enp1s0
  sudo ip route add 10.0.0.5 dev enp1s0
  ```

Конфигурация netplan (для установки сети при запуске)
```yaml
# /etc/netplan/01-network-manager-all.yaml
# Let NetworkManager manage all devices on this system
network:
  version: 2
  renderer: NetworkManager
  ethernets:
    enp1s0d1:
      dhcp4: false
      addresses: [10.0.0.4/24]
      routes:
      - to: 10.0.0.6/32
      - to: 10.0.0.5/32
```

### Тесты с OpenVPN

ip link add name br0 type bridge
ip link set dev br0 up
ip link set dev enp1s0 master br0
ip link set dev tun0 master br0


iptables -t nat -A PREROUTING -i tun0 -d 10.8.0.3 -j DNAT --to-destination 10.0.0.6


iptables -t nat -D PREROUTING -i tun0 -d 10.8.0.3 -j DNAT --to-destination 10.0.0.6


iptables -t nat -A POSTROUTING -o enp1s0 -d 10.8.0.3 -j MASQUERADE

iptables -t nat -D POSTROUTING -o enp1s0 -d 10.8.0.3 -j MASQUERADE


iptables -t mangle -A PREROUTING -i eth0 -p udp --dport 53 -j TEE --gateway 192.168.0.10

---
DevFlashProg ошибка mysql
sudo apt-get install libqt5sql5-mysql