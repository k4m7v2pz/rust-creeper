package com.github.games647.lambdaattack.bot;

import com.github.games647.lambdaattack.LambdaAttack;
import com.github.games647.lambdaattack.Options;
import com.github.games647.lambdaattack.UniversalFactory;
import com.github.games647.lambdaattack.UniversalProtocol;
import com.github.games647.lambdaattack.bot.listener.*;
import org.geysermc.mcprotocollib.auth.GameProfile;
import org.geysermc.mcprotocollib.network.ProxyInfo;
import org.geysermc.mcprotocollib.network.Session;
import org.geysermc.mcprotocollib.network.tcp.TcpClientSession;

import java.util.logging.Logger;

public class Bot {

    public static final char COMMAND_IDENTIFIER = '/';

    private final Options options;
    private final ProxyInfo proxyInfo;
    private final Logger logger;
    private final UniversalProtocol account;

    private Session session;
    private EntitiyLocation location;
    private float health = -1;
    private float food = -1;

    public Bot(Options options, UniversalProtocol account) {
        this(options, account, null);
    }

    public Bot(Options options, UniversalProtocol account, ProxyInfo proxyInfo) {
        this.options = options;
        this.account = account;
        this.proxyInfo = proxyInfo;

        this.logger = Logger.getLogger(account.getProfile().getName());
        this.logger.setParent(LambdaAttack.getLogger());
    }

    public void connect(String host, int port) {
        TcpClientSession client;
        if (proxyInfo == null) {
            client = new TcpClientSession(host, port, account.getProtocol());
        } else {
            client = new TcpClientSession(host, port, account.getProtocol(), proxyInfo);
        }
        this.session = client;

        switch (account.getGameVersion()) {
            case VERSION_1_21_1:
                client.addListener(new SessionListener1211(options, this));
                break;
            default:
                throw new IllegalStateException("Unknown session listener");
        }

        client.connect();
    }

    public void sendMessage(String message) {
        if (session != null) {
            UniversalFactory.sendChatMessage(account.getGameVersion(), message, getSession());
        }
    }

    public boolean isOnline() {
        return session != null && session.isConnected();
    }

    public Session getSession() {
        return session;
    }

    public EntitiyLocation getLocation() {
        return location;
    }

    public void setLocation(EntitiyLocation location) {
        this.location = location;
    }

    public double getHealth() {
        return health;
    }

    public void setHealth(float health) {
        this.health = health;
    }

    public float getFood() {
        return food;
    }

    public void setFood(float food) {
        this.food = food;
    }

    public Logger getLogger() {
        return logger;
    }

    public GameProfile getGameProfile() {
        return account.getProfile();
    }

    public ProxyInfo getProxy() {
        return proxyInfo;
    }

    public void disconnect() {
        if (session != null) {
            session.disconnect("Disconnect");
        }
    }
}
