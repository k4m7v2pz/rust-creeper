package com.github.games647.lambdaattack;

import org.geysermc.mcprotocollib.auth.GameProfile;
import org.geysermc.mcprotocollib.network.packet.PacketProtocol;

public interface UniversalProtocol {

    GameProfile getProfile();

    PacketProtocol getProtocol();

    GameVersion getGameVersion();
}
