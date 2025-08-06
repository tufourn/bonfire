use anyhow::Result;
use std::sync::Arc;

use ash::vk;

use super::device;

pub struct CommandRingBufferBuilder {
    num_threads: usize,
    primary_buffers_per_pool: usize,
    secondary_buffers_per_pool: usize,
    queue: device::Queue,
    device: Arc<device::Device>,
}

impl CommandRingBufferBuilder {
    pub fn new(device: Arc<device::Device>) -> Self {
        Self {
            num_threads: 1,
            primary_buffers_per_pool: 1,
            secondary_buffers_per_pool: 0,
            queue: device.graphics_queue,
            device,
        }
    }

    pub fn build(self) -> Result<CommandRingBuffer> {
        assert!(self.num_threads > 0, "Must have at least 1 thread");
        assert!(
            self.primary_buffers_per_pool > 0,
            "Must have at least 1 primary buffer per pool"
        );
        assert!(
            self.primary_buffers_per_pool <= u8::MAX as usize,
            "Too many primary buffers"
        );
        assert!(
            self.secondary_buffers_per_pool <= u8::MAX as usize,
            "Too many secondary buffers"
        );

        let pool_create_info = vk::CommandPoolCreateInfo::default()
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
            .queue_family_index(self.queue.family);

        let num_pools = self.num_threads * device::FRAMES_IN_FLIGHT;

        let mut command_pools = Vec::with_capacity(num_pools);
        for _ in 0..num_pools {
            let command_pool = unsafe {
                self.device
                    .raw
                    .create_command_pool(&pool_create_info, None)?
            };
            command_pools.push(command_pool);
        }

        let mut primary_buffers = Vec::with_capacity(num_pools * self.primary_buffers_per_pool);
        let mut secondary_buffers = Vec::with_capacity(num_pools * self.secondary_buffers_per_pool);
        for command_pool in &command_pools {
            let primary_alloc_info = vk::CommandBufferAllocateInfo::default()
                .command_pool(*command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(self.primary_buffers_per_pool as u32);
            let mut pool_primary_buffers = unsafe {
                self.device
                    .raw
                    .allocate_command_buffers(&primary_alloc_info)?
            };
            primary_buffers.append(&mut pool_primary_buffers);

            if self.secondary_buffers_per_pool > 0 {
                let secondary_alloc_info = vk::CommandBufferAllocateInfo::default()
                    .command_pool(*command_pool)
                    .level(vk::CommandBufferLevel::SECONDARY)
                    .command_buffer_count(self.secondary_buffers_per_pool as u32);
                let mut pool_secondary_buffers = unsafe {
                    self.device
                        .raw
                        .allocate_command_buffers(&secondary_alloc_info)?
                };
                secondary_buffers.append(&mut pool_secondary_buffers);
            }
        }

        let used_primary_buffers = vec![0; num_pools];
        let used_secondary_buffers = vec![0; num_pools];

        Ok(CommandRingBuffer {
            device: self.device,
            command_pools,
            primary_buffers_per_pool: self.primary_buffers_per_pool as u8,
            used_primary_offset: used_primary_buffers,
            secondary_buffers_per_pool: self.secondary_buffers_per_pool as u8,
            used_secondary_offset: used_secondary_buffers,
            primary_buffers,
            secondary_buffers,
        })
    }

    pub fn queue(mut self, queue: device::Queue) -> Self {
        self.queue = queue;
        self
    }

    pub fn num_pools(mut self, pool_count: usize) -> Self {
        self.num_threads = pool_count;
        self
    }

    pub fn primary_buffers_per_pool(mut self, buffer_count: usize) -> Self {
        self.primary_buffers_per_pool = buffer_count;
        self
    }

    pub fn secondary_buffers_per_pool(mut self, buffer_count: usize) -> Self {
        self.secondary_buffers_per_pool = buffer_count;
        self
    }
}

pub struct CommandRingBuffer {
    device: Arc<device::Device>,

    command_pools: Vec<vk::CommandPool>,

    primary_buffers: Vec<vk::CommandBuffer>,
    secondary_buffers: Vec<vk::CommandBuffer>,

    used_primary_offset: Vec<u8>,
    used_secondary_offset: Vec<u8>,

    primary_buffers_per_pool: u8,
    secondary_buffers_per_pool: u8,
}

impl CommandRingBuffer {
    pub fn builder(device: Arc<device::Device>) -> CommandRingBufferBuilder {
        CommandRingBufferBuilder::new(device)
    }

    pub fn reset_pool(&mut self, thread_index: usize) -> Result<()> {
        let pool_index = Self::pool_from_indices(self.device.frame_index(), thread_index);
        unsafe {
            self.device.raw.reset_command_pool(
                self.command_pools[pool_index],
                vk::CommandPoolResetFlags::empty(),
            )?
        };

        self.used_primary_offset[pool_index] = 0;
        self.used_secondary_offset[pool_index] = 0;

        Ok(())
    }

    pub fn get_next_primary_buffer(&mut self, thread_index: usize) -> vk::CommandBuffer {
        let pool_index = Self::pool_from_indices(self.device.frame_index(), thread_index);
        assert!(
            self.used_primary_offset[pool_index] < self.primary_buffers_per_pool,
            "Out of primary command buffer"
        );
        let cmd_index = pool_index * self.primary_buffers_per_pool as usize
            + self.used_primary_offset[pool_index] as usize;
        let cmd = self.primary_buffers[cmd_index];
        self.used_primary_offset[pool_index] += 1;

        cmd
    }

    pub fn get_next_secondary_buffer(&mut self, thread_index: usize) -> vk::CommandBuffer {
        let pool_index = Self::pool_from_indices(self.device.frame_index(), thread_index);
        assert!(
            self.used_secondary_offset[pool_index] < self.secondary_buffers_per_pool,
            "Out of secondary command buffer"
        );
        let cmd_index = pool_index * self.secondary_buffers_per_pool as usize
            + self.used_secondary_offset[pool_index] as usize;
        let cmd = self.secondary_buffers[cmd_index];
        self.used_secondary_offset[pool_index] += 1;

        cmd
    }

    fn pool_from_indices(frame_index: usize, thread_index: usize) -> usize {
        thread_index * device::FRAMES_IN_FLIGHT + frame_index
    }
}

impl Drop for CommandRingBuffer {
    fn drop(&mut self) {
        unsafe {
            for &pool in &self.command_pools {
                self.device.raw.destroy_command_pool(pool, None);
            }
        }
    }
}
