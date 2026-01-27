# XDP DNS-Drop Test Guide (veth + namespace)

## Setup assumptions

* Host interface: `veth0`
* Namespace: `ns1`
* Host IP: `10.200.1.1`
* XDP object: `./target/bpfel-unknown-none/release/af_xdp`
* XDP section: `xdp`
* Netcat: OpenBSD (`-k` supported)

---

## 1️⃣ Start a DNS listener (host)

```bash
sudo nc -u -l -k -p 53 -s 10.200.1.1
```

Leave this running.

---

## 2️⃣ Baseline test (NO XDP)

```bash
sudo ip link set dev veth0 xdp off
sudo ip netns exec ns1 sh -c 'echo "DNS_NO_XDP" | nc -u -w1 10.200.1.1 53'
```

✅ **Expected:** `DNS_NO_XDP` appears
❌ If not: stop — listener or veth/ns setup is wrong

---

## 3️⃣ Attach XDP

```bash
sudo ip link set dev veth0 xdp obj ./target/bpfel-unknown-none/release/af_xdp sec xdp
ip -details link show dev veth0 | head -n 6
```

✅ Must show `prog/xdp id …`

---

## 4️⃣ Test DNS with XDP (DROP expected)

```bash
sudo ip netns exec ns1 sh -c 'echo "DNS_WITH_XDP" | nc -u -w1 10.200.1.1 53'
```

✅ **Expected:** nothing appears
→ DNS packet dropped by XDP

---

## 5️⃣ Verify with tcpdump (optional but definitive)

```bash
sudo tcpdump -ni veth0 udp port 53
```

Send again from `ns1`.

* tcpdump sees packet + nc sees nothing → **XDP drop confirmed**
* tcpdump sees nothing → packet never reached interface

---

## 6️⃣ Detach XDP and re-test

```bash
sudo ip link set dev veth0 xdp off
sudo ip netns exec ns1 sh -c 'echo "DNS_AFTER_DETACH" | nc -u -w1 10.200.1.1 53'
```

✅ **Expected:** `DNS_AFTER_DETACH` appears

---

## ✔️ What this proves

* XDP is attached correctly
* DNS (UDP/53) is dropped **only** when XDP is active
* Other traffic (e.g. UDP/9999) passes normally

---

## Quick reset

```bash
sudo ip link set dev veth0 xdp off
```

---
