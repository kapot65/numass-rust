return function(frame: Frame)
    for _, channel in frame do
        host.hpf(channel, 2)
    end
end