package com.cloudwebrtc.webrtc;

import android.content.Context;
import android.util.Log;

import java.net.InetAddress;
import java.net.NetworkInterface;
import java.util.ArrayList;
import java.util.Collections;
import java.util.HashSet;
import java.util.List;
import java.util.Set;

import org.webrtc.NetworkChangeDetector;
import org.webrtc.NetworkMonitor;
import org.webrtc.NetworkMonitorAutoDetect;

/**
 * wado patch: the networks libwebrtc may use are the ones Android's ConnectivityManager lists,
 * plus this phone's own hotspot (or USB / Bluetooth tethering), which it does not list.
 *
 * Without this a computer on the phone's hotspot was reachable only out through the carrier and
 * back — never across the hotspot itself, the one path that works. libwebrtc binds each socket to
 * its Android network by handle; an interface it was never told about has none, so the bind fails
 * and no UDP candidate is made there. Reported here with handle 0, the binder answers "not
 * implemented" and the socket is bound to the hotspot address the plain way.
 */
final class TetherAwareDetector implements NetworkChangeDetector {
  private static final String TAG = "TetherAwareDetector";
  private final NetworkMonitorAutoDetect inner;

  private TetherAwareDetector(NetworkChangeDetector.Observer observer, Context context) {
    inner = new NetworkMonitorAutoDetect(observer, context);
  }

  /** Before the first peer connection factory: the monitor builds its detector from this. */
  static void install() {
    NetworkMonitor.getInstance().setNetworkChangeDetectorFactory(TetherAwareDetector::new);
  }

  @Override
  public ConnectionType getCurrentConnectionType() {
    return inner.getCurrentConnectionType();
  }

  @Override
  public boolean supportNetworkCallback() {
    return inner.supportNetworkCallback();
  }

  @Override
  public void destroy() {
    inner.destroy();
  }

  @Override
  public List<NetworkInformation> getActiveNetworkList() {
    List<NetworkInformation> known = inner.getActiveNetworkList();
    List<NetworkInformation> out = known == null ? new ArrayList<>() : new ArrayList<>(known);
    Set<String> names = new HashSet<>();
    for (NetworkInformation n : out) names.add(n.name);
    try {
      for (NetworkInterface ni : Collections.list(NetworkInterface.getNetworkInterfaces())) {
        String name = ni.getName();
        if (!ni.isUp() || ni.isLoopback() || names.contains(name) || !tethering(name)) continue;
        List<IPAddress> ips = new ArrayList<>();
        for (InetAddress a : Collections.list(ni.getInetAddresses())) {
          if (a.isLinkLocalAddress() || a.isLoopbackAddress()) continue;
          ips.add(new IPAddress(a.getAddress()));
        }
        if (ips.isEmpty()) continue;
        out.add(new NetworkInformation(name, ConnectionType.CONNECTION_WIFI, ConnectionType.CONNECTION_NONE,
            0, ips.toArray(new IPAddress[0]), NetworkSlice.NO_SLICE));
        Log.d(TAG, "hotspot network " + name + " with " + ips.size() + " address(es)");
      }
    } catch (Exception e) {
      Log.w(TAG, "could not list interfaces", e);
    }
    return out;
  }

  /**
   * An interface the phone serves others on, not one it reaches the world through: the carrier's
   * (rmnet, ccmni, its CLAT v4-…), VPN tunnels and Wi-Fi Direct stay with ConnectivityManager.
   */
  private static boolean tethering(String name) {
    for (String skip : new String[] {"rmnet", "ccmni", "v4-", "tun", "dummy", "ip6", "sit", "p2p", "ifb", "lo", "umts", "seth", "clat"}) {
      if (name.startsWith(skip)) return false;
    }
    return true;
  }
}
