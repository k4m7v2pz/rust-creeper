package com.github.games647.lambdaattack.version.v1_21_1;

import com.github.games647.lambdaattack.GameVersion;
import com.github.games647.lambdaattack.UniversalProtocol;
import org.geysermc.mcprotocollib.auth.GameProfile;
import org.geysermc.mcprotocollib.network.packet.PacketProtocol;
import org.geysermc.mcprotocollib.protocol.MinecraftProtocol;

import java.util.UUID;

public class ProtocolWrapper extends MinecraftProtocol implements UniversalProtocol {
    private final GameProfile profile;

    public ProtocolWrapper(String username) {
        super(username);
        this.profile = new GameProfile(UUID.randomUUID(), username);
    }

    @Override
    public PacketProtocol getProtocol() {
        return this;
    }

    @Override
    public GameVersion getGameVersion() {
        return GameVersion.VERSION_1_21_1;
    }

    @Override
    public GameProfile getProfile() {
        return profile;
    }
}
