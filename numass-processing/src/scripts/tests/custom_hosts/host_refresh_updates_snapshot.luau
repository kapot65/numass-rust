return function(frame: Frame)
    for _, channel in frame do
        host.double(channel)
        host.refresh(channel)
        channel.samples[1] = channel.samples[1] + 100
    end
end
