#![allow(dead_code)]
use super::command::{Command, ComputeDistanceCode, InitCommand};
use super::static_dict::{BROTLI_UNALIGNED_LOAD32, BROTLI_UNALIGNED_LOAD64, FindMatchLengthWithLimit};
use super::static_dict::BrotliDictionary;
use super::super::alloc;
use super::super::alloc::{SliceWrapper, SliceWrapperMut};
use super::util::{Log2FloorNonZero, brotli_max_size_t};
use core;
static kBrotliMinWindowBits: i32 = 10i32;

static kBrotliMaxWindowBits: i32 = 24i32;

static kInvalidMatch: u32 = 0xfffffffu32;

static kCutoffTransformsCount: u32 = 10u32;

static kCutoffTransforms: u64 = 0x71b520au64 << 32i32 | 0xda2d3200u32 as (u64);

pub static kHashMul32: u32 = 0x1e35a7bdu32;

pub static kHashMul64: u64 = 0x1e35a7bdu64 << 32i32 | 0x1e35a7bdu64;

pub static kHashMul64Long: u64 = 0x1fe35a7bu32 as (u64) << 32i32 | 0xd3579bd3u32 as (u64);



pub enum BrotliEncoderMode {
  BROTLI_MODE_GENERIC = 0,
  BROTLI_MODE_TEXT = 1,
  BROTLI_MODE_FONT = 2,
}


#[derive(Clone,Copy)]
pub struct BrotliHasherParams {
  pub type_: i32,
  pub bucket_bits: i32,
  pub block_bits: i32,
  pub hash_len: i32,
  pub num_last_distances_to_check: i32,
}



pub struct BrotliEncoderParams {
  pub mode: BrotliEncoderMode,
  pub quality: i32,
  pub lgwin: i32,
  pub lgblock: i32,
  pub size_hint: usize,
  pub disable_literal_context_modeling: i32,
  pub hasher: BrotliHasherParams,
}



fn LiteralSpreeLengthForSparseSearch(params: &BrotliEncoderParams) -> usize {
  (if (*params).quality < 9i32 {
     64i32
   } else {
     512i32
   }) as (usize)
}

fn brotli_min_size_t(a: usize, b: usize) -> usize {
  if a < b { a } else { b }
}

pub enum HowPrepared {
    ALREADY_PREPARED,
    NEWLY_PREPARED,
}

pub struct HasherSearchResult {
  pub len: usize,
  pub len_x_code: usize,
  pub distance: usize,
  pub score: usize,
}

pub struct Struct1 {
  pub params: BrotliHasherParams,
  pub is_prepared_: i32,
  pub dict_num_lookups: usize,
  pub dict_num_matches: usize,
}

pub trait AnyHasher {
  fn GetHasherCommon(&mut self) -> &mut Struct1;
  fn HashBytes(&self, data: &[u8]) -> usize;
  fn HashTypeLength(&self) -> usize;
  fn StoreLookahead(&self) -> usize;
  fn PrepareDistanceCache(&self, distance_cache: &mut [i32]);
  fn FindLongestMatch(&mut self,
                      dictionary: &BrotliDictionary,
                      dictionary_hash: &[u16],
                      data: &[u8],
                      ring_buffer_mask: usize,
                      distance_cache: &[i32],
                      cur_ix: usize,
                      max_length: usize,
                      max_backward: usize,
                      out: &mut HasherSearchResult)
                      -> bool;
  fn Store(&mut self, data: &[u8], mask: usize, ix: usize);
  fn StoreRange(&mut self, data: &[u8], mask: usize, ix_start: usize, ix_end: usize);
  fn Prepare(&mut self, one_shot: bool, input_size:usize, data:&[u8]) -> HowPrepared;
  fn StitchToPreviousBlock(&mut self,
                           num_bytes: usize,
                           position: usize,
                           ringbuffer: &[u8],
                           ringbuffer_mask: usize);
}

pub fn StitchToPreviousBlockInternal<T:AnyHasher>(mut handle: &mut T,
                                          num_bytes: usize,
                                          position: usize,
                                          ringbuffer: &[u8],
                                          ringbuffer_mask: usize) {
    if num_bytes >= handle.HashTypeLength().wrapping_sub(1) && (position >= 3) {
        handle.Store(ringbuffer, ringbuffer_mask, position.wrapping_sub(3));
        handle.Store(ringbuffer, ringbuffer_mask, position.wrapping_sub(2));
        handle.Store(ringbuffer, ringbuffer_mask, position.wrapping_sub(1));
    }
}

pub fn StoreLookaheadThenStore<T:AnyHasher>(mut hasher: &mut T, size: usize, dict:&[u8]) {
    let overlap = hasher.StoreLookahead().wrapping_sub(1usize);
    let mut i :usize = 0;
    while i.wrapping_add(overlap) < size {
        hasher.Store(dict, !(0usize), i);
        i = i.wrapping_add(1 as (usize));
    }
}

pub trait BasicHashComputer {
  fn HashBytes(&self, data: &[u8]) -> u32;
  fn BUCKET_BITS(&self) -> i32;
  fn USE_DICTIONARY(&self) -> i32;
  fn BUCKET_SWEEP(&self) -> i32;
}
pub struct BasicHasher<Buckets: SliceWrapperMut<u32> + SliceWrapper<u32> + BasicHashComputer> {
  pub GetHasherCommon: Struct1,
  pub buckets_: Buckets,
}
pub struct H2Sub {
  pub buckets_: [u32; 65537],
}
impl<T: SliceWrapperMut<u32> + SliceWrapper<u32> + BasicHashComputer> AnyHasher for BasicHasher<T> {
  #[allow(unused_variables)]
  fn PrepareDistanceCache(&self, distance_cache: &mut [i32]) {}
  fn HashTypeLength(&self) -> usize {
    8
  }
  fn StoreLookahead(&self) -> usize {
    8
  }
  fn StitchToPreviousBlock(&mut self,
                           num_bytes: usize,
                           position: usize,
                           ringbuffer: &[u8],
                           ringbuffer_mask: usize){
      StitchToPreviousBlockInternal(self,
                                    num_bytes,
                                    position,
                                    ringbuffer,
                                    ringbuffer_mask);
  }
  fn GetHasherCommon(&mut self) -> &mut Struct1 {
    return &mut self.GetHasherCommon;
  }
  fn HashBytes(&self, data: &[u8]) -> usize {
    self.buckets_.HashBytes(data) as usize
  }
  fn Store(&mut self, data: &[u8], mask: usize, ix: usize) {
    let key: u32 = self.HashBytes(&data[((ix & mask) as (usize))..]) as u32;
    let off: u32 = (ix >> 3i32).wrapping_rem(self.buckets_.BUCKET_SWEEP() as usize) as (u32);
    self.buckets_.slice_mut()[key.wrapping_add(off) as (usize)] = ix as (u32);
  }
  fn StoreRange(&mut self, data: &[u8], mask: usize, ix_start: usize, ix_end: usize) {
    let mut i: usize;
    i = ix_start;
    while i < ix_end {
      {
        self.Store(data, mask, i);
      }
      i = i.wrapping_add(1 as (usize));
    }
  }
  fn Prepare(&mut self, one_shot: bool, input_size:usize, data:&[u8]) -> HowPrepared {
      if self.GetHasherCommon.is_prepared_ != 0 {
          return HowPrepared::ALREADY_PREPARED;
      }
      let partial_prepare_threshold = (4 << self.buckets_.BUCKET_BITS()) >> 7;
      if one_shot && input_size <= partial_prepare_threshold {
        for i in 0..input_size {
            let key = self.HashBytes(&data[i..]) as usize;
            let bs = self.buckets_.BUCKET_SWEEP() as usize;
            for item in self.buckets_.slice_mut()[key..(key + bs)].iter_mut() {
                *item = 0;
            }
        }
      } else {
        for item in self.buckets_.slice_mut().iter_mut() {
          *item =0;
        }
      }
      self.GetHasherCommon.is_prepared_ = 1;
      HowPrepared::NEWLY_PREPARED
  }

  fn FindLongestMatch(&mut self,
                      dictionary: &BrotliDictionary,
                      dictionary_hash: &[u16],
                      data: &[u8],
                      ring_buffer_mask: usize,
                      distance_cache: &[i32],
                      cur_ix: usize,
                      max_length: usize,
                      max_backward: usize,
                      mut out: &mut HasherSearchResult)
                      -> bool {
    let best_len_in: usize = (*out).len;
    let cur_ix_masked: usize = cur_ix & ring_buffer_mask;
    let key: u32 = self.HashBytes(&data[(cur_ix_masked as (usize))..]) as u32;
    let mut compare_char: i32 = data[(cur_ix_masked.wrapping_add(best_len_in) as (usize))] as (i32);
    let mut best_score: usize = (*out).score;
    let mut best_len: usize = best_len_in;
    let cached_backward: usize = distance_cache[(0usize)] as (usize);
    let mut prev_ix: usize = cur_ix.wrapping_sub(cached_backward);
    let mut is_match_found: i32 = 0i32;
    (*out).len_x_code = 0usize;
    if prev_ix < cur_ix {
      prev_ix = prev_ix & ring_buffer_mask as (u32) as (usize);
      if compare_char == data[(prev_ix.wrapping_add(best_len) as (usize))] as (i32) {
        let len: usize = FindMatchLengthWithLimit(&data[(prev_ix as (usize))..],
                                                  &data[(cur_ix_masked as (usize))..],
                                                  max_length);
        if len >= 4usize {
          best_score = BackwardReferenceScoreUsingLastDistance(len);
          best_len = len;
          (*out).len = len;
          (*out).distance = cached_backward;
          (*out).score = best_score;
          compare_char = data[(cur_ix_masked.wrapping_add(best_len) as (usize))] as (i32);
          if self.buckets_.BUCKET_SWEEP() == 1i32 {
            (*self).buckets_.slice_mut()[key as (usize)] = cur_ix as (u32);
            return true;
          } else {
            is_match_found = 1i32;
          }
        }
      }
    }
    let BUCKET_SWEEP = self.buckets_.BUCKET_SWEEP();
    if BUCKET_SWEEP == 1i32 {
      let backward: usize;
      let len: usize;
      prev_ix = (*self).buckets_.slice()[key as (usize)] as (usize);
      (*self).buckets_.slice_mut()[key as (usize)] = cur_ix as (u32);
      backward = cur_ix.wrapping_sub(prev_ix);
      prev_ix = prev_ix & ring_buffer_mask as (u32) as (usize);
      if compare_char != data[(prev_ix.wrapping_add(best_len_in) as (usize))] as (i32) {
        return false;
      }
      if backward == 0usize || backward > max_backward {
        return false;
      }
      len = FindMatchLengthWithLimit(&data[(prev_ix as (usize))..],
                                     &data[(cur_ix_masked as (usize))..],
                                     max_length);
      if len >= 4usize {
        (*out).len = len;
        (*out).distance = backward;
        (*out).score = BackwardReferenceScore(len, backward);
        return true;
      }
    } else {
      let (old_, mut bucket) = (*self).buckets_.slice_mut()[key as usize..].split_at_mut(1);
      let mut i: i32;
      prev_ix = old_[0] as (usize);
      i = 0i32;

      while i < BUCKET_SWEEP {
        'continue3: loop {
          {
            let backward: usize = cur_ix.wrapping_sub(prev_ix);
            let len: usize;
            prev_ix = prev_ix & ring_buffer_mask as (u32) as (usize);
            if compare_char != data[(prev_ix.wrapping_add(best_len) as (usize))] as (i32) {
              {
                break 'continue3;
              }
            }
            if backward == 0usize || backward > max_backward {
              {
                break 'continue3;
              }
            }
            len = FindMatchLengthWithLimit(&data[(prev_ix as (usize))..],
                                           &data[(cur_ix_masked as (usize))..],
                                           max_length);
            if len >= 4usize {
              let score: usize = BackwardReferenceScore(len, backward);
              if best_score < score {
                best_score = score;
                best_len = len;
                (*out).len = best_len;
                (*out).distance = backward;
                (*out).score = score;
                compare_char = data[(cur_ix_masked.wrapping_add(best_len) as (usize))] as (i32);
                is_match_found = 1i32;
              }
            }
          }
          break;
        }
        i = i + 1;
        {
          let (_old, new_bucket) = core::mem::replace(&mut bucket, &mut []).split_at_mut(1);
          prev_ix = _old[0] as usize;
          bucket = new_bucket;
        }
      }
    }
    if self.buckets_.USE_DICTIONARY() != 0 && (is_match_found == 0) {
      is_match_found = SearchInStaticDictionary(dictionary,
                                                dictionary_hash,
                                                self,
                                                &data[(cur_ix_masked as (usize))..],
                                                max_length,
                                                max_backward,
                                                out,
                                                1i32);
    }
    (*self).buckets_.slice_mut()[(key as (usize)).wrapping_add((cur_ix >> 3)
                                    .wrapping_rem(self.buckets_.BUCKET_SWEEP() as
                                                  usize))] = cur_ix as (u32);
    is_match_found != 0

  }
}
impl BasicHashComputer for H2Sub {
  fn HashBytes(&self, data: &[u8]) -> u32 {
    let h: u64 = (BROTLI_UNALIGNED_LOAD64(data) << 64i32 - 8i32 * 5i32).wrapping_mul(kHashMul64);
    (h >> 64i32 - 16i32) as (u32)
  }
  fn BUCKET_BITS(&self) -> i32 {
    16
  }
  fn BUCKET_SWEEP(&self) -> i32 {
    1
  }
  fn USE_DICTIONARY(&self) -> i32 {
    1
  }
}
impl SliceWrapperMut<u32> for H2Sub {
  fn slice_mut(&mut self) -> &mut [u32] {
    return &mut self.buckets_[..];
  }
}
impl SliceWrapper<u32> for H2Sub {
  fn slice(&self) -> &[u32] {
    return &self.buckets_[..];
  }
}
pub struct H3Sub {
  pub buckets_: [u32; 65538],
}
impl SliceWrapperMut<u32> for H3Sub {
  fn slice_mut(&mut self) -> &mut [u32] {
    return &mut self.buckets_[..];
  }
}
impl SliceWrapper<u32> for H3Sub {
  fn slice(&self) -> &[u32] {
    return &self.buckets_[..];
  }
}
impl BasicHashComputer for H3Sub {
  fn BUCKET_BITS(&self) -> i32 {
    16
  }
  fn BUCKET_SWEEP(&self) -> i32 {
    2
  }
  fn USE_DICTIONARY(&self) -> i32 {
    0
  }
  fn HashBytes(&self, data: &[u8]) -> u32 {
    let h: u64 = (BROTLI_UNALIGNED_LOAD64(data) << 64i32 - 8i32 * 5i32).wrapping_mul(kHashMul64);
    (h >> 64i32 - 16i32) as (u32)
  }
}
pub struct H4Sub {
  pub buckets_: [u32; 131076],
}
impl BasicHashComputer for H4Sub {
  fn BUCKET_BITS(&self) -> i32 {
    17
  }
  fn BUCKET_SWEEP(&self) -> i32 {
    4
  }
  fn USE_DICTIONARY(&self) -> i32 {
    1
  }
  fn HashBytes(&self, data: &[u8]) -> u32 {
    let h: u64 = (BROTLI_UNALIGNED_LOAD64(data) << 64i32 - 8i32 * 5i32).wrapping_mul(kHashMul64);
    (h >> 64i32 - 17i32) as (u32)
  }
}
impl SliceWrapperMut<u32> for H4Sub {
  fn slice_mut(&mut self) -> &mut [u32] {
    return &mut self.buckets_[..];
  }
}
impl SliceWrapper<u32> for H4Sub {
  fn slice(&self) -> &[u32] {
    return &self.buckets_[..];
  }
}
pub struct H54Sub {
  pub buckets_: [u32; 1048580],
}
impl BasicHashComputer for H54Sub {
  fn BUCKET_BITS(&self) -> i32 {
    20
  }
  fn BUCKET_SWEEP(&self) -> i32 {
    4
  }
  fn USE_DICTIONARY(&self) -> i32 {
    0
  }
  fn HashBytes(&self, data: &[u8]) -> u32 {
    let h: u64 = (BROTLI_UNALIGNED_LOAD64(data) << 64i32 - 8i32 * 7i32).wrapping_mul(kHashMul64);
    (h >> 64i32 - 20i32) as (u32)
  }
}

impl SliceWrapperMut<u32> for H54Sub {
  fn slice_mut(&mut self) -> &mut [u32] {
    return &mut self.buckets_[..];
  }
}
impl SliceWrapper<u32> for H54Sub {
  fn slice(&self) -> &[u32] {
    return &self.buckets_[..];
  }
}
pub trait AdvHashSpecialization {
  fn get_hash_mask(&self) -> u64;
  fn set_hash_mask(&mut self, params_hash_len: i32);
  fn HashTypeLength(&self) -> usize;
  fn StoreLookahead(&self) -> usize;
}

pub struct AdvHasher<Specialization: AdvHashSpecialization + Sized,
                     AllocU16: alloc::Allocator<u16>,
                     AllocU32: alloc::Allocator<u32>>
{
  pub GetHasherCommon: Struct1,
  pub bucket_size_: u64,
  pub block_size_: u64,
  pub specialization: Specialization, // contains hash_mask_
  pub hash_shift_: i32,
  pub block_mask_: u32,
  pub num: AllocU16::AllocatedMemory,
  pub buckets: AllocU32::AllocatedMemory,
}
pub struct H5Sub {}
impl AdvHashSpecialization for H5Sub {
  fn get_hash_mask(&self) -> u64 {
    return 0xffffffffffffffffu64;
  }
  #[allow(unused_variables)]
  fn set_hash_mask(&mut self, params_hash_len: i32) {}
  fn HashTypeLength(&self) -> usize {
    4
  }
  fn StoreLookahead(&self) -> usize {
    4
  }
}

pub struct H6Sub {
  pub hash_mask: u64,
}

impl AdvHashSpecialization for H6Sub {
  fn get_hash_mask(&self) -> u64 {
    self.hash_mask
  }
  fn set_hash_mask(&mut self, params_hash_len: i32) {
    self.hash_mask = !(0u32 as (u64)) >> 64i32 - 8i32 * params_hash_len;
  }
  fn HashTypeLength(&self) -> usize {
    8
  }
  fn StoreLookahead(&self) -> usize {
    8
  }
}

fn BackwardReferencePenaltyUsingLastDistance(distance_short_code: usize) -> usize {
  (39usize).wrapping_add((0x1ca10i32 >> (distance_short_code & 0xeusize) & 0xei32) as (usize))
}


impl<Specialization: AdvHashSpecialization, AllocU16: alloc::Allocator<u16>, AllocU32: alloc::Allocator<u32>> AnyHasher
  for AdvHasher<Specialization, AllocU16, AllocU32> {
  fn PrepareDistanceCache(&self, mut distance_cache: &mut [i32]){
    let num_distances = self.GetHasherCommon.params.num_last_distances_to_check;
    if num_distances > 4i32 {
      let last_distance: i32 = distance_cache[(0usize)];
      distance_cache[(4usize)] = last_distance - 1i32;
      distance_cache[(5usize)] = last_distance + 1i32;
      distance_cache[(6usize)] = last_distance - 2i32;
      distance_cache[(7usize)] = last_distance + 2i32;
      distance_cache[(8usize)] = last_distance - 3i32;
      distance_cache[(9usize)] = last_distance + 3i32;
      if num_distances > 10i32 {
        let next_last_distance: i32 = distance_cache[(1usize)];
        distance_cache[(10usize)] = next_last_distance - 1i32;
        distance_cache[(11usize)] = next_last_distance + 1i32;
        distance_cache[(12usize)] = next_last_distance - 2i32;
        distance_cache[(13usize)] = next_last_distance + 2i32;
        distance_cache[(14usize)] = next_last_distance - 3i32;
        distance_cache[(15usize)] = next_last_distance + 3i32;
      }
    }
  }
  fn StitchToPreviousBlock(&mut self,
                           num_bytes: usize,
                           position: usize,
                           ringbuffer: &[u8],
                           ringbuffer_mask: usize) {
      StitchToPreviousBlockInternal(self,
                                    num_bytes,
                                    position,
                                    ringbuffer,
                                    ringbuffer_mask);
  }
  fn Prepare(&mut self, one_shot: bool, input_size:usize, data:&[u8]) ->HowPrepared {
      if self.GetHasherCommon.is_prepared_ != 0 {
          return HowPrepared::ALREADY_PREPARED;
      }
      let partial_prepare_threshold = self.bucket_size_ as usize >> 6;
      if one_shot && input_size <= partial_prepare_threshold {
        for i in 0..input_size {
          let key = self.HashBytes(&data[i..]);
          self.num.slice_mut()[key] = 0;
        }
      } else {
        for item in self.num.slice_mut()[..(self.bucket_size_ as usize)].iter_mut() {
          *item =0;
        }
      }
      self.GetHasherCommon.is_prepared_ = 1;
      HowPrepared::NEWLY_PREPARED
  }

  fn GetHasherCommon(&mut self) -> &mut Struct1 {
    &mut self.GetHasherCommon
  }
  fn HashTypeLength(&self) -> usize {
     self.specialization.HashTypeLength()
  }
  fn StoreLookahead(&self) -> usize {
     self.specialization.StoreLookahead()
  }
  fn HashBytes(&self, data: &[u8]) -> usize {
    let mask = self.specialization.get_hash_mask();
    let shift = self.hash_shift_;
    let h: u64 = (BROTLI_UNALIGNED_LOAD64(data) & mask).wrapping_mul(kHashMul64Long);
    (h >> shift) as (u32) as usize
  }
  fn Store(&mut self, data: &[u8], mask: usize, ix: usize) {
    let key: u32 = self.HashBytes(&data[((ix & mask) as (usize))..]) as u32;
    let minor_ix: usize = (self.num.slice()[(key as (usize))] as (u32) & (*self).block_mask_) as (usize);
    let offset: usize = minor_ix.wrapping_add((key << (self.GetHasherCommon).params.block_bits) as
                                              (usize));
    self.buckets.slice_mut()[offset] = ix as (u32);
    {
      let _rhs = 1;
      let _lhs = &mut self.num.slice_mut()[(key as (usize))];
      *_lhs = (*_lhs as (i32) + _rhs) as (u16);
    }
  }
  fn StoreRange(&mut self, data: &[u8], mask: usize, ix_start: usize, ix_end: usize) {
    let mut i: usize;
    i = ix_start;
    while i < ix_end {
      {
        self.Store(data, mask, i);
      }
      i = i.wrapping_add(1 as (usize));
    }
  }

  fn FindLongestMatch(&mut self,
                      dictionary: &BrotliDictionary,
                      dictionary_hash: &[u16],
                      data: &[u8],
                      ring_buffer_mask: usize,
                      distance_cache: &[i32],
                      cur_ix: usize,
                      max_length: usize,
                      max_backward: usize,
                      mut out: &mut HasherSearchResult)
                      -> bool {
    let cur_ix_masked: usize = cur_ix & ring_buffer_mask;
    let mut is_match_found: i32 = 0i32;
    let mut best_score: usize = (*out).score;
    let mut best_len: usize = (*out).len;
    let mut i: usize;
    (*out).len = 0usize;
    (*out).len_x_code = 0usize;
    i = 0usize;
    while i < self.GetHasherCommon.params.num_last_distances_to_check as (usize) {
      'continue45: loop {
        {
          let backward: usize = distance_cache[(i as (usize))] as (usize);
          let mut prev_ix: usize = cur_ix.wrapping_sub(backward);
          if prev_ix >= cur_ix {
            {
              break 'continue45;
            }
          }
          if backward > max_backward {
            {
              break 'continue45;
            }
          }
          prev_ix = prev_ix & ring_buffer_mask;
          if cur_ix_masked.wrapping_add(best_len) > ring_buffer_mask || prev_ix.wrapping_add(best_len) > ring_buffer_mask ||
             data[(cur_ix_masked.wrapping_add(best_len) as (usize))] as (i32) !=
             data[(prev_ix.wrapping_add(best_len) as (usize))] as (i32) {
            {
              break 'continue45;
            }
          }
          {
            let len: usize = FindMatchLengthWithLimit(&data[(prev_ix as (usize))..],
                                                      &data[(cur_ix_masked as (usize))..],
                                                      max_length);
            if len >= 3usize || len == 2usize && (i < 2usize) {
              let mut score: usize = BackwardReferenceScoreUsingLastDistance(len);
              if best_score < score {
                if i != 0usize {
                  score = score.wrapping_sub(BackwardReferencePenaltyUsingLastDistance(i));
                }
                if best_score < score {
                  best_score = score;
                  best_len = len;
                  (*out).len = best_len;
                  (*out).distance = backward;
                  (*out).score = best_score;
                  is_match_found = 1i32;
                }
              }
            }
          }
        }
        break;
      }
      i = i.wrapping_add(1 as (usize));
    }
    {
      let key: u32 = self.HashBytes(&data[(cur_ix_masked as (usize))..]) as u32;
      let common_block_bits = self.GetHasherCommon.params.block_bits;
      let mut bucket: &mut [u32] = &mut self.buckets.slice_mut()[((key << common_block_bits) as (usize))..];
      let down: usize = if self.num.slice()[(key as (usize))] as (u64) > (*self).block_size_ {
        (self.num.slice()[(key as (usize))] as (u64)).wrapping_sub((*self).block_size_) as usize
      } else {
        0u32 as (usize)
      };
      i = self.num.slice()[(key as (usize))] as (usize);
      while i > down {
        let mut prev_ix: usize = bucket[(({
            i = i.wrapping_sub(1 as (usize));
            i
          } & (*self).block_mask_ as (usize)) as (usize))] as (usize);
        let backward: usize = cur_ix.wrapping_sub(prev_ix);
        if backward > max_backward {
          {
            break;
          }
        }
        prev_ix = prev_ix & ring_buffer_mask;
        if cur_ix_masked.wrapping_add(best_len) > ring_buffer_mask || prev_ix.wrapping_add(best_len) > ring_buffer_mask ||
           data[(cur_ix_masked.wrapping_add(best_len) as (usize))] as (i32) !=
           data[(prev_ix.wrapping_add(best_len) as (usize))] as (i32) {
          {
            continue;
          }
        }
        {
          let len: usize = FindMatchLengthWithLimit(&data[(prev_ix as (usize))..],
                                                    &data[(cur_ix_masked as (usize))..],
                                                    max_length);
          if len >= 4usize {
            let score: usize = BackwardReferenceScore(len, backward);
            if best_score < score {
              best_score = score;
              best_len = len;
              (*out).len = best_len;
              (*out).distance = backward;
              (*out).score = best_score;
              is_match_found = 1i32;
            }
          }
        }
      }
      bucket[((self.num.slice()[(key as (usize))] as (u32) & (self).block_mask_) as (usize))] = cur_ix as (u32);
      {
        let _rhs = 1;
        let _lhs = &mut self.num.slice_mut()[(key as (usize))];
        *_lhs = (*_lhs as (i32) + _rhs) as (u16);
      }
    }
    if is_match_found == 0 {
      is_match_found = SearchInStaticDictionary(dictionary,
                                                dictionary_hash,
                                                self,
                                                &data[(cur_ix_masked as (usize))..],
                                                max_length,
                                                max_backward,
                                                out,
                                                0i32);
    }
    is_match_found != 0

  }
}


pub struct BankH40 {
  pub slots: [SlotH40; 65536],
}

pub struct BankH41 {
  pub slots: [SlotH41; 65536],
}

pub struct BankH42 {
  pub slots: [SlotH42; 512],
}


pub struct SlotH40 {
  pub delta: u16,
  pub next: u16,
}
pub struct SlotH41 {
  pub delta: u16,
  pub next: u16,
}

pub struct SlotH42 {
  pub delta: u16,
  pub next: u16,
}

// UNSUPPORTED, for now.
pub struct H40 {
  pub common: Struct1,
  pub addr: [u32; 32768],
  pub head: [u16; 32768],
  pub tiny_hash: [u8; 65536],
  pub banks: [BankH40; 1],
  pub free_slot_idx: [u16; 1],
  pub max_hops: usize,
}


pub struct H41 {
  pub common: Struct1,
  pub addr: [u32; 32768],
  pub head: [u16; 32768],
  pub tiny_hash: [u8; 65536],
  pub banks: [BankH41; 1],
  pub free_slot_idx: [u16; 1],
  pub max_hops: usize,
}

pub struct H42 {
  pub common: Struct1,
  pub addr: [u32; 32768],
  pub head: [u16; 32768],
  pub tiny_hash: [u8; 65536],
  pub banks: [BankH42; 512],
  free_slot_idx: [u16; 512],
  pub max_hops: usize,
}




fn unopt_ctzll(mut val: usize) -> u8 {
  let mut cnt: u8 = 0i32 as (u8);
  while val & 1usize == 0usize {
    val = val >> 1i32;
    cnt = (cnt as (i32) + 1) as (u8);
  }
  cnt
}


fn BackwardReferenceScoreUsingLastDistance(copy_length: usize) -> usize {
  (135usize)
    .wrapping_mul(copy_length)
    .wrapping_add(((30i32 * 8i32) as (usize)).wrapping_mul(::core::mem::size_of::<usize>()))
    .wrapping_add(15usize)
}


fn BackwardReferenceScore(copy_length: usize, backward_reference_offset: usize) -> usize {
  ((30i32 * 8i32) as (usize))
    .wrapping_mul(::core::mem::size_of::<usize>())
    .wrapping_add((135usize).wrapping_mul(copy_length))
    .wrapping_sub((30u32).wrapping_mul(Log2FloorNonZero(backward_reference_offset as u64)) as (usize))
}

fn Hash14(data: &[u8]) -> u32 {
  let h: u32 = BROTLI_UNALIGNED_LOAD32(data).wrapping_mul(kHashMul32);
  h >> 32i32 - 14i32
}

fn TestStaticDictionaryItem(dictionary: &BrotliDictionary,
                            item: usize,
                            data: &[u8],
                            max_length: usize,
                            max_backward: usize,
                            mut out: &mut HasherSearchResult)
                            -> i32 {
  let len: usize;
  let dist: usize;
  let offset: usize;
  let matchlen: usize;
  let backward: usize;
  let score: usize;
  len = item & 0x1fusize;
  dist = item >> 5i32;
  offset = ((*dictionary).offsets_by_length[len] as (usize)).wrapping_add(len.wrapping_mul(dist));
  if len > max_length {
    return 0i32;
  }
  matchlen = FindMatchLengthWithLimit(data, &(*dictionary).data[offset..], len);
  if matchlen.wrapping_add(kCutoffTransformsCount as usize) <= len || matchlen == 0usize {
    return 0i32;
  }
  {
    let cut: usize = len.wrapping_sub(matchlen);
    let transform_id: usize =
      (cut << 2i32).wrapping_add(kCutoffTransforms as usize >> cut.wrapping_mul(6) & 0x3f);
    backward = max_backward.wrapping_add(dist)
      .wrapping_add(1usize)
      .wrapping_add(transform_id << (*dictionary).size_bits_by_length[len] as (i32));
  }
  score = BackwardReferenceScore(matchlen, backward);
  if score < (*out).score {
    return 0i32;
  }
  (*out).len = matchlen;
  (*out).len_x_code = len ^ matchlen;
  (*out).distance = backward;
  (*out).score = score;
  1i32
}

fn SearchInStaticDictionary<HasherType: AnyHasher>(dictionary: &BrotliDictionary,
                                                   dictionary_hash: &[u16],
                                                   mut handle: &mut HasherType,
                                                   data: &[u8],
                                                   max_length: usize,
                                                   max_backward: usize,
                                                   mut out: &mut HasherSearchResult,
                                                   shallow: i32)
                                                   -> i32 {
  let mut key: usize;
  let mut i: usize;
  let mut is_match_found: i32 = 0i32;
  let mut xself: &mut Struct1 = handle.GetHasherCommon();
  if (*xself).dict_num_matches < (*xself).dict_num_lookups >> 7i32 {
    return 0i32;
  }
  key = (Hash14(data) << 1i32) as (usize); //FIXME: works for any kind of hasher??
  i = 0usize;
  while i < if shallow != 0 { 1u32 } else { 2u32 } as (usize) {
    {
      let item: usize = dictionary_hash[(key as (usize))] as (usize);
      (*xself).dict_num_lookups = (*xself).dict_num_lookups.wrapping_add(1 as (usize));
      if item != 0usize {
        let item_matches: i32 =
          TestStaticDictionaryItem(dictionary, item, data, max_length, max_backward, out);
        if item_matches != 0 {
          (*xself).dict_num_matches = (*xself).dict_num_matches.wrapping_add(1 as (usize));
          is_match_found = 1i32;
        }
      }
    }
    i = i.wrapping_add(1 as (usize));
    key = key.wrapping_add(1 as (usize));
  }
  is_match_found
}

pub enum UnionHasher<AllocU16: alloc::Allocator<u16>,
                 AllocU32: alloc::Allocator<u32>> {
    Uninit,
    H2(BasicHasher<H2Sub>),
    H3(BasicHasher<H3Sub>),
    H4(BasicHasher<H4Sub>),
    H54(BasicHasher<H54Sub>),
    H5(AdvHasher<H5Sub, AllocU16, AllocU32>),
    H6(AdvHasher<H6Sub, AllocU16, AllocU32>),
}
macro_rules! match_all_hashers_mut {
    ($xself : expr, $func_call : ident, $( $args:expr),*) => {
        match $xself {
     &mut UnionHasher::H2(ref mut hasher) => hasher.$func_call($($args),*),
     &mut UnionHasher::H3(ref mut hasher) => hasher.$func_call($($args),*),
     &mut UnionHasher::H4(ref mut hasher) => hasher.$func_call($($args),*),
     &mut UnionHasher::H5(ref mut hasher) => hasher.$func_call($($args),*),
     &mut UnionHasher::H6(ref mut hasher) => hasher.$func_call($($args),*),
     &mut UnionHasher::H54(ref mut hasher) => hasher.$func_call($($args),*),
     Uninit => panic!("UNINTIALIZED"),
        }
    };
}
macro_rules! match_all_hashers {
    ($xself : expr, $func_call : ident, $( $args:expr),*) => {
        match $xself {
     &UnionHasher::H2(ref hasher) => hasher.$func_call($($args),*),
     &UnionHasher::H3(ref hasher) => hasher.$func_call($($args),*),
     &UnionHasher::H4(ref hasher) => hasher.$func_call($($args),*),
     &UnionHasher::H5(ref hasher) => hasher.$func_call($($args),*),
     &UnionHasher::H6(ref hasher) => hasher.$func_call($($args),*),
     & UnionHasher::H54(ref hasher) => hasher.$func_call($($args),*),
     Uninit => panic!("UNINTIALIZED"),
        }
    };
}
impl<AllocU16: alloc::Allocator<u16>,
      AllocU32: alloc::Allocator<u32>> AnyHasher for UnionHasher<AllocU16, AllocU32> {
  fn GetHasherCommon(&mut self) -> &mut Struct1 {
     return match_all_hashers_mut!(self, GetHasherCommon,);
  }
  fn Prepare(&mut self, one_shot: bool, input_size:usize, data:&[u8]) -> HowPrepared {
      return match_all_hashers_mut!(self, Prepare, one_shot, input_size, data);
  }
  fn HashBytes(&self, data: &[u8]) -> usize {
     return match_all_hashers!(self, HashBytes, data);
  }
  fn HashTypeLength(&self) -> usize{
     return match_all_hashers!(self, HashTypeLength,);
  }
  fn StoreLookahead(&self) -> usize{
     return match_all_hashers!(self, StoreLookahead,);
  }
  fn PrepareDistanceCache(&self, distance_cache: &mut [i32]){
     return match_all_hashers!(self, PrepareDistanceCache, distance_cache);
  }
  fn StitchToPreviousBlock(&mut self,
                           num_bytes: usize,
                           position: usize,
                           ringbuffer: &[u8],
                           ringbuffer_mask: usize) {
    return match_all_hashers_mut!(self, StitchToPreviousBlock,
                              num_bytes,
                              position,
                              ringbuffer,
                              ringbuffer_mask);
  }
  fn FindLongestMatch(&mut self,
                      dictionary: &BrotliDictionary,
                      dictionary_hash: &[u16],
                      data: &[u8],
                      ring_buffer_mask: usize,
                      distance_cache: &[i32],
                      cur_ix: usize,
                      max_length: usize,
                      max_backward: usize,
                      out: &mut HasherSearchResult)
                      -> bool{
     return match_all_hashers_mut!(self, FindLongestMatch, dictionary, dictionary_hash, data, ring_buffer_mask, distance_cache, cur_ix, max_length, max_backward, out);
  }
  fn Store(&mut self, data: &[u8], mask: usize, ix: usize){
     return match_all_hashers_mut!(self, Store, data, mask, ix);
  }
  fn StoreRange(&mut self, data: &[u8], mask: usize, ix_start: usize, ix_end: usize){
     return match_all_hashers_mut!(self, StoreRange, data, mask, ix_start, ix_end);
  }
}
impl<AllocU16: alloc::Allocator<u16>,
                 AllocU32: alloc::Allocator<u32>> Default for UnionHasher<AllocU16, AllocU32> {
                 fn default() -> Self {
    UnionHasher::Uninit
}
}

/*UnionHasher::H2(BasicHasher {
          GetHasherCommon:Struct1{params:BrotliHasherParams{
           type_:2,
           block_bits: 8,
           bucket_bits:16,
           hash_len: 4,
           num_last_distances_to_check:0},
          is_prepared_:0,
          dict_num_lookups:0,
          dict_num_matches:0,
          },
          buckets_:H2Sub{
          buckets_:[0;65537],
          },
          })
          */
fn CreateBackwardReferences<AH: AnyHasher>(dictionary: &BrotliDictionary,
                                           dictionary_hash: &[u16],
                                           num_bytes: usize,
                                           mut position: usize,
                                           ringbuffer: &[u8],
                                           ringbuffer_mask: usize,
                                           params: &BrotliEncoderParams,
                                           mut hasher: &mut AH,
                                           mut dist_cache: &mut [i32],
                                           mut last_insert_len: &mut usize,
                                           mut commands: &mut [Command],
                                           mut num_commands: &mut usize,
                                           mut num_literals: &mut usize) {
  let max_backward_limit: usize = (1usize << (*params).lgwin).wrapping_sub(16usize);
  let mut new_commands_count: usize = 0;
  let mut insert_length: usize = *last_insert_len;
  let pos_end: usize = position.wrapping_add(num_bytes);
  let store_end: usize = if num_bytes >= hasher.StoreLookahead() {
    position.wrapping_add(num_bytes).wrapping_sub(hasher.StoreLookahead()).wrapping_add(1usize)
  } else {
    position
  };
  let random_heuristics_window_size: usize = LiteralSpreeLengthForSparseSearch(params);
  let mut apply_random_heuristics: usize = position.wrapping_add(random_heuristics_window_size);
  let kMinScore: usize = ((30i32 * 8i32) as (usize))
    .wrapping_mul(::core::mem::size_of::<usize>())
    .wrapping_add(100usize);
  hasher.PrepareDistanceCache(dist_cache);
  while position.wrapping_add(hasher.HashTypeLength()) < pos_end {
    let mut max_length: usize = pos_end.wrapping_sub(position);
    let mut max_distance: usize = brotli_min_size_t(position, max_backward_limit);
    let mut sr = HasherSearchResult {
      len: 0,
      len_x_code: 0,
      distance: 0,
      score: 0,
    };
    sr.len = 0usize;
    sr.len_x_code = 0usize;
    sr.distance = 0usize;
    sr.score = kMinScore;
    if hasher.FindLongestMatch(dictionary,
                               dictionary_hash,
                               ringbuffer,
                               ringbuffer_mask,
                               dist_cache,
                               position,
                               max_length,
                               max_distance,
                               &mut sr) {
      let mut delayed_backward_references_in_row: i32 = 0i32;
      max_length = max_length.wrapping_sub(1 as (usize));
      'break6: loop {
        'continue7: loop {
          let cost_diff_lazy: usize = 175usize;
          let is_match_found: bool;
          let mut sr2 = HasherSearchResult {
            len: 0,
            len_x_code: 0,
            distance: 0,
            score: 0,
          };
          sr2.len = if (*params).quality < 5i32 {
            brotli_min_size_t(sr.len.wrapping_sub(1usize), max_length)
          } else {
            0usize
          };
          sr2.len_x_code = 0usize;
          sr2.distance = 0usize;
          sr2.score = kMinScore;
          max_distance = brotli_min_size_t(position.wrapping_add(1usize), max_backward_limit);
          is_match_found = hasher.FindLongestMatch(dictionary,
                                                   dictionary_hash,
                                                   ringbuffer,
                                                   ringbuffer_mask,
                                                   dist_cache,
                                                   position.wrapping_add(1usize),
                                                   max_length,
                                                   max_distance,
                                                   &mut sr2);
          if is_match_found && (sr2.score >= sr.score.wrapping_add(cost_diff_lazy)) {
            position = position.wrapping_add(1 as (usize));
            insert_length = insert_length.wrapping_add(1 as (usize));
            sr = sr2;
            if {
                 delayed_backward_references_in_row = delayed_backward_references_in_row + 1;
                 delayed_backward_references_in_row
               } < 4i32 &&
               (position.wrapping_add(hasher.HashTypeLength()) < pos_end) {
              {
                break 'continue7;
              }
            }
          }
          break 'break6;
        }
        max_length = max_length.wrapping_sub(1 as (usize));
      }
      apply_random_heuristics = position.wrapping_add((2usize).wrapping_mul(sr.len))
        .wrapping_add(random_heuristics_window_size);
      max_distance = brotli_min_size_t(position, max_backward_limit);
      {
        let distance_code: usize = ComputeDistanceCode(sr.distance, max_distance, dist_cache);
        if sr.distance <= max_distance && (distance_code > 0usize) {
          dist_cache[(3usize)] = dist_cache[(2usize)];
          dist_cache[(2usize)] = dist_cache[(1usize)];
          dist_cache[(1usize)] = dist_cache[(0usize)];
          dist_cache[(0usize)] = sr.distance as (i32);
          hasher.PrepareDistanceCache(dist_cache);
        }
        new_commands_count += 1;
        InitCommand({
                      let (mut _old, new_commands) = core::mem::replace(&mut commands, &mut []).split_at_mut(1);
                      commands = new_commands;
                      &mut _old[0]
                    },
                    insert_length,
                    sr.len,
                    sr.len ^ sr.len_x_code,
                    distance_code);
      }
      *num_literals = (*num_literals).wrapping_add(insert_length);
      insert_length = 0usize;
      hasher.StoreRange(ringbuffer,
                        ringbuffer_mask,
                        position.wrapping_add(2usize),
                        brotli_min_size_t(position.wrapping_add(sr.len), store_end));
      position = position.wrapping_add(sr.len);
    } else {
      insert_length = insert_length.wrapping_add(1 as (usize));
      position = position.wrapping_add(1 as (usize));
      if position > apply_random_heuristics {
        if position >
           apply_random_heuristics.wrapping_add((4usize)
                                                  .wrapping_mul(random_heuristics_window_size)) {
          let kMargin: usize = brotli_max_size_t(hasher.StoreLookahead().wrapping_sub(1usize),
                                                 4usize);
          let pos_jump: usize = brotli_min_size_t(position.wrapping_add(16usize),
                                                  pos_end.wrapping_sub(kMargin));
          while position < pos_jump {
            {
              hasher.Store(ringbuffer, ringbuffer_mask, position);
              insert_length = insert_length.wrapping_add(4usize);
            }
            position = position.wrapping_add(4usize);
          }
        } else {
          let kMargin: usize = brotli_max_size_t(hasher.StoreLookahead().wrapping_sub(1usize),
                                                 2usize);
          let pos_jump: usize = brotli_min_size_t(position.wrapping_add(8usize),
                                                  pos_end.wrapping_sub(kMargin));
          while position < pos_jump {
            {
              hasher.Store(ringbuffer, ringbuffer_mask, position);
              insert_length = insert_length.wrapping_add(2usize);
            }
            position = position.wrapping_add(2usize);
          }
        }
      }
    }
  }
  insert_length = insert_length.wrapping_add(pos_end.wrapping_sub(position));
  *last_insert_len = insert_length;
  *num_commands = (*num_commands).wrapping_add(new_commands_count);
}
macro_rules! call_brotli_create_backward_references {
    () => {
        CreateBackwardReferences(dictionary, dictionary_hash, num_bytes, position,
                                 ringbuffer, ringbuffer_mask,
                                 params,
                                 hasher,
                                 dist_cache,
                                 last_insert_len,
                                 commands,
                                 num_commands,
                                 num_literals);
    };
}
pub fn BrotliCreateBackwardReferences<AllocU16: alloc::Allocator<u16>,
                                      AllocU32: alloc::Allocator<u32>>(dictionary: &BrotliDictionary,
                                           dictionary_hash: &[u16],
                                           num_bytes: usize,
                                           position: usize,
                                           ringbuffer: &[u8],
                                           ringbuffer_mask: usize,
                                           params: &BrotliEncoderParams,
                                           mut hasher_union: &mut UnionHasher<AllocU16, AllocU32>,
                                           mut dist_cache: &mut [i32],
                                           mut last_insert_len: &mut usize,
                                           mut commands: &mut [Command],
                                           mut num_commands: &mut usize,
                                                                       mut num_literals: &mut usize) {
    match(hasher_union) {
        &mut UnionHasher::Uninit => panic!("working with uninitialized hash map"),
        &mut UnionHasher::H2(ref mut hasher) =>
        CreateBackwardReferences(dictionary, dictionary_hash, num_bytes, position,
                                 ringbuffer, ringbuffer_mask,
                                 params,
                                 hasher,
                                 dist_cache,
                                 last_insert_len,
                                 commands,
                                 num_commands,
                                 num_literals),
//     call_brotli_create_backward_references!(),
        &mut UnionHasher::H3(ref mut hasher) =>
        CreateBackwardReferences(dictionary, dictionary_hash, num_bytes, position,
                                 ringbuffer, ringbuffer_mask,
                                 params,
                                 hasher,
                                 dist_cache,
                                 last_insert_len,
                                 commands,
                                 num_commands,
                                 num_literals),
//            call_brotli_create_backward_references!(),
        &mut UnionHasher::H4(ref mut hasher) =>
        CreateBackwardReferences(dictionary, dictionary_hash, num_bytes, position,
                                 ringbuffer, ringbuffer_mask,
                                 params,
                                 hasher,
                                 dist_cache,
                                 last_insert_len,
                                 commands,
                                 num_commands,
                                 num_literals),
//            call_brotli_create_backward_references!(),
        &mut UnionHasher::H5(ref mut hasher) =>
        CreateBackwardReferences(dictionary, dictionary_hash, num_bytes, position,
                                 ringbuffer, ringbuffer_mask,
                                 params,
                                 hasher,
                                 dist_cache,
                                 last_insert_len,
                                 commands,
                                 num_commands,
                                 num_literals),
//            call_brotli_create_backward_references!(),
        &mut UnionHasher::H6(ref mut hasher) =>
        CreateBackwardReferences(dictionary, dictionary_hash, num_bytes, position,
                                 ringbuffer, ringbuffer_mask,
                                 params,
                                 hasher,
                                 dist_cache,
                                 last_insert_len,
                                 commands,
                                 num_commands,
                                 num_literals),
//            call_brotli_create_backward_references!(),
        &mut UnionHasher::H54(ref mut hasher) =>
        CreateBackwardReferences(dictionary, dictionary_hash, num_bytes, position,
                                 ringbuffer, ringbuffer_mask,
                                 params,
                                 hasher,
                                 dist_cache,
                                 last_insert_len,
                                 commands,
                                 num_commands,
                                 num_literals),
//            call_brotli_create_backward_references!(),
    }
}