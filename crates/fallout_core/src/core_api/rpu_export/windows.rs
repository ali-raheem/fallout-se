//! Local NTFS only. Every lookup after opening the volume root is relative to a
//! retained directory handle, with OBJ_DONT_REPARSE. Ancestors deny delete sharing.
//! Publication renames the OWNED staging handle, with ReplaceIfExists=false.
//! No path-based removal, overwrite fallback, or directory creation exists.
//! This does not defend against a hostile kernel/admin or authenticate a source
//! pathname supplied by the caller. Once handles close, other actors may change
//! the output. Directory-entry crash durability is not claimed.
use super::*;
use std::{
    fs::File,
    mem::{offset_of, size_of},
    os::windows::io::{AsRawHandle, FromRawHandle},
    ptr,
};
use windows_sys::{
    Wdk::{
        Foundation::OBJECT_ATTRIBUTES,
        Storage::FileSystem::{
            FILE_CREATE, FILE_DIRECTORY_FILE, FILE_NON_DIRECTORY_FILE, FILE_OPEN,
            FILE_OPEN_REPARSE_POINT, FILE_RENAME_INFORMATION, FILE_SYNCHRONOUS_IO_NONALERT,
            FileRenameInformation, NtCreateFile, NtSetInformationFile,
        },
    },
    Win32::{
        Foundation::{
            HANDLE, INVALID_HANDLE_VALUE, OBJ_CASE_INSENSITIVE, OBJ_DONT_REPARSE,
            RtlNtStatusToDosError, UNICODE_STRING,
        },
        Storage::FileSystem::*,
        System::IO::IO_STATUS_BLOCK,
    },
};

fn handle(file: &File) -> HANDLE {
    file.as_raw_handle()
}
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
fn last(op: &'static str) -> ExportError {
    ExportError::io(op, io::Error::last_os_error())
}
fn info(file: &File) -> Result<BY_HANDLE_FILE_INFORMATION, ExportError> {
    let mut value = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: live File handle and initialized output storage.
    if unsafe { GetFileInformationByHandle(handle(file), &mut value) } == 0 {
        return Err(last("file identity"));
    }
    if value.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(ExportError::UnsafeRedirection);
    }
    Ok(value)
}
fn same_file(a: &File, b: &File) -> Result<bool, ExportError> {
    let a = info(a)?;
    let b = info(b)?;
    Ok((a.dwVolumeSerialNumber, a.nFileIndexHigh, a.nFileIndexLow)
        == (b.dwVolumeSerialNumber, b.nFileIndexHigh, b.nFileIndexLow))
}
fn relative(
    parent: &File,
    name: &str,
    directory: bool,
    create: bool,
    writable: bool,
    share: u32,
) -> Result<File, ExportError> {
    let diagnostic_name = name.to_owned();
    let name = wide(name);
    let length = u16::try_from((name.len() - 1) * 2).map_err(|_| ExportError::InvalidPath)?;
    let unicode = UNICODE_STRING {
        Length: length,
        MaximumLength: length + 2,
        Buffer: name.as_ptr().cast_mut(),
    };
    let attributes = OBJECT_ATTRIBUTES {
        Length: size_of::<OBJECT_ATTRIBUTES>() as u32,
        RootDirectory: handle(parent),
        ObjectName: &unicode,
        Attributes: OBJ_CASE_INSENSITIVE | OBJ_DONT_REPARSE,
        ..Default::default()
    };
    let mut result = ptr::null_mut();
    let mut status = IO_STATUS_BLOCK::default();
    let access = if directory {
        FILE_READ_ATTRIBUTES | FILE_TRAVERSE | SYNCHRONIZE
    } else if writable {
        FILE_GENERIC_READ | FILE_GENERIC_WRITE | DELETE
    } else {
        FILE_GENERIC_READ
    };
    // SAFETY: all pointers remain live through this synchronous call. Only an
    // exclusively created staging file ever receives write/delete access.
    let code = unsafe {
        NtCreateFile(
            &mut result,
            access,
            &attributes,
            &mut status,
            ptr::null(),
            FILE_ATTRIBUTE_NORMAL,
            share,
            if create { FILE_CREATE } else { FILE_OPEN },
            FILE_SYNCHRONOUS_IO_NONALERT
                | FILE_OPEN_REPARSE_POINT
                | if directory {
                    FILE_DIRECTORY_FILE
                } else {
                    FILE_NON_DIRECTORY_FILE
                },
            ptr::null(),
            0,
        )
    };
    if code < 0 {
        // SAFETY: status is the NTSTATUS returned by NtCreateFile.
        let code = unsafe { RtlNtStatusToDosError(code) };
        return Err(ExportError::Io {
            operation: "relative open",
            code: Some(code as i32),
            message: format!(
                "{diagnostic_name}: {}",
                io::Error::from_raw_os_error(code as i32)
            ),
        });
    }
    // SAFETY: successful NtCreateFile transferred one owned handle.
    let file = unsafe { File::from_raw_handle(result) };
    // FILE_CREATE + FILE_NON_DIRECTORY_FILE + FILE_ATTRIBUTE_NORMAL constructed
    // this new regular file. Its caller must retain ownership before any further
    // fallible checks, so even a metadata-query failure gets handle cleanup.
    if create {
        return Ok(file);
    }
    let attributes = info(&file)?.dwFileAttributes;
    if (attributes & FILE_ATTRIBUTE_DIRECTORY != 0) != directory {
        return Err(ExportError::UnsafeRedirection);
    }
    Ok(file)
}
fn absent(error: &ExportError) -> bool {
    matches!(
        error,
        ExportError::Io {
            code: Some(2 | 3),
            ..
        }
    )
}
fn check_name(file: &File, expected: &str) -> Result<(), ExportError> {
    let mut name = [0u16; 512];
    // SAFETY: bounded writable UTF-16 output, live handle.
    let len =
        unsafe { GetFinalPathNameByHandleW(handle(file), name.as_mut_ptr(), name.len() as u32, 0) };
    if len == 0 {
        return Err(last("canonical handle path"));
    }
    if len as usize >= name.len() {
        return Err(ExportError::InvalidPath);
    }
    let value = String::from_utf16(&name[..len as usize]).map_err(|_| ExportError::InvalidPath)?;
    if !value
        .trim_end_matches('\\')
        .eq_ignore_ascii_case(&format!("\\\\?\\{expected}").trim_end_matches('\\'))
    {
        return Err(ExportError::UnsafeRedirection);
    }
    Ok(())
}
struct Parent {
    chain: Vec<File>,
    path: String,
    name: String,
}
impl Parent {
    fn last(&self) -> &File {
        self.chain.last().expect("volume root always retained")
    }
    fn open(path: &CheckedPath, destination: bool) -> Result<Self, ExportError> {
        let rootname = wide(&format!("\\\\?\\{}", path.drive));
        let drive = wide(&path.drive);
        // DRIVE_FIXED = 3. Restriction avoids remote/share semantics entirely.
        if unsafe { GetDriveTypeW(drive.as_ptr()) } != 3 {
            return Err(ExportError::UnsupportedFilesystem);
        }
        // SAFETY: terminated path, valid flags, null optional pointers.
        let root = unsafe {
            CreateFileW(
                rootname.as_ptr(),
                FILE_READ_ATTRIBUTES | FILE_TRAVERSE | SYNCHRONIZE,
                FILE_SHARE_READ,
                ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                ptr::null_mut(),
            )
        };
        if root == INVALID_HANDLE_VALUE {
            return Err(last("open volume root"));
        }
        // SAFETY: this is the sole owner of the newly opened handle.
        let root = unsafe { File::from_raw_handle(root) };
        info(&root)?;
        check_name(&root, &path.drive)?;
        let mut fs = [0u16; 32];
        // SAFETY: optional outputs null, filesystem output bounded.
        if unsafe {
            GetVolumeInformationByHandleW(
                handle(&root),
                ptr::null_mut(),
                0,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                fs.as_mut_ptr(),
                fs.len() as u32,
            )
        } == 0
        {
            return Err(last("filesystem type"));
        }
        if &fs[..5] != [b'N' as u16, b'T' as u16, b'F' as u16, b'S' as u16, 0] {
            return Err(ExportError::UnsupportedFilesystem);
        }
        let mut owner = Self {
            chain: vec![root],
            path: path.drive.clone(),
            name: path
                .components
                .last()
                .ok_or(ExportError::InvalidPath)?
                .clone(),
        };
        if destination {
            owner.protected()?;
        }
        for part in &path.components[..path.components.len() - 1] {
            let next = relative(owner.last(), part, true, false, false, FILE_SHARE_READ)?;
            owner.path.push_str(part);
            owner.path.push('\\');
            check_name(&next, &owner.path)?;
            owner.chain.push(next);
            if destination {
                owner.protected()?;
            }
        }
        Ok(owner)
    }
    fn protected(&self) -> Result<(), ExportError> {
        // Only exact safety marker lookups within explicitly selected ancestors.
        // Never discover resources or infer load order from these filenames.
        for marker in [
            "fallout2.exe",
            "fallout2.cfg",
            "fallout.exe",
            "fallout.cfg",
            "master.dat",
            "critter.dat",
            "ddraw.ini",
        ] {
            match relative(self.last(), marker, false, false, false, FILE_SHARE_READ) {
                Ok(_) => return Err(ExportError::ProtectedDestination),
                Err(e) if absent(&e) => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}
fn cleanup(file: &File) -> Result<(), ExportError> {
    let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
    // SAFETY: DELETE access belongs only to this operation's staging handle.
    // This never resolves a pathname and cannot delete a swapped-in file.
    if unsafe {
        SetFileInformationByHandle(
            handle(file),
            FileDispositionInfo,
            (&disposition as *const FILE_DISPOSITION_INFO).cast(),
            size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
    } == 0
    {
        return Err(last("cleanup owned staging handle"));
    }
    Ok(())
}
fn rename_no_replace(file: &File, name: &str) -> Result<(), ExportError> {
    let name = wide(name);
    let offset = offset_of!(FILE_RENAME_INFORMATION, FileName);
    let size = offset
        .checked_add(name.len() * 2)
        .ok_or(ExportError::InvalidPath)?;
    let mut aligned = vec![0usize; size.div_ceil(size_of::<usize>())];
    let data = aligned.as_mut_ptr().cast::<FILE_RENAME_INFORMATION>();
    let mut status = IO_STATUS_BLOCK::default();
    // SAFETY: usize allocation aligns FILE_RENAME_INFORMATION on supported Windows
    // targets and has room for the variable-length terminated UTF-16 filename.
    // ReplaceIfExists FALSE is essential: collision must fail without replacement.
    unsafe {
        (*data).Anonymous.ReplaceIfExists = false;
        // Native same-directory rename: simple filename, NULL RootDirectory.
        // The destination is relative to the STAGING FILE'S directory, never
        // process cwd. That directory remains held throughout the operation.
        (*data).RootDirectory = ptr::null_mut();
        (*data).FileNameLength = ((name.len() - 1) * 2) as u32;
        ptr::copy_nonoverlapping(
            name.as_ptr(),
            ptr::addr_of_mut!((*data).FileName).cast::<u16>(),
            name.len(),
        );
        let code = NtSetInformationFile(
            handle(file),
            &mut status,
            data.cast(),
            size as u32,
            FileRenameInformation,
        );
        if code < 0 {
            return Err(ExportError::io(
                "no-replace publication",
                io::Error::from_raw_os_error(RtlNtStatusToDosError(code) as i32),
            ));
        }
    }
    Ok(())
}

pub(super) fn publish(
    copy: &ExperimentalCopy,
    current: &ByteSnapshot,
    env: &RpuSourcePlan,
    diagnostic: &RpuDiagnosticReport,
    source: &CheckedPath,
    destination: &CheckedPath,
    faults: &Faults,
) -> Result<(), ExportFailure> {
    let (_source_parent, _source_file, parent) = open_context(copy, source, destination)?;
    publish_staged(copy, current, env, diagnostic, &parent, faults)
}

pub(super) fn preflight(
    copy: &ExperimentalCopy,
    source: &CheckedPath,
    destination: &CheckedPath,
) -> Result<(), ExportFailure> {
    open_context(copy, source, destination).map(|_| ())
}

fn open_context(
    copy: &ExperimentalCopy,
    source: &CheckedPath,
    destination: &CheckedPath,
) -> Result<(Parent, File, Parent), ExportFailure> {
    let source_parent = Parent::open(source, false)?;
    let mut source_file = relative(
        source_parent.last(),
        &source_parent.name,
        false,
        false,
        false,
        FILE_SHARE_READ,
    )?;
    check_name(&source_file, &source.full)?;
    compare(
        &read_stream(&mut source_file, copy.baseline().bytes().len())?,
        copy.baseline().bytes(),
    )
    .map_err(|_| ExportError::SourceMismatch)?;
    let parent = Parent::open(destination, true)?;
    match relative(
        parent.last(),
        &parent.name,
        false,
        false,
        false,
        FILE_SHARE_READ,
    ) {
        Ok(file) => {
            return Err(if same_file(&file, &source_file)? {
                ExportError::SourceAlias
            } else {
                ExportError::ExistingDestination
            }
            .into());
        }
        Err(e) if absent(&e) => {}
        Err(e) => return Err(e.into()),
    }
    Ok((source_parent, source_file, parent))
}

fn publish_staged(
    copy: &ExperimentalCopy,
    current: &ByteSnapshot,
    env: &RpuSourcePlan,
    diagnostic: &RpuDiagnosticReport,
    parent: &Parent,
    faults: &Faults,
) -> Result<(), ExportFailure> {
    // Globally unique process-local identity + exclusive creation. Never open,
    // truncate, reuse or delete a pre-existing staging pathname.
    let stage_name = format!(
        ".rpu-stage-{}-{:?}.tmp",
        std::process::id(),
        UniqueId::fresh()
    )
    .replace(['(', ')'], "");
    let stage_path = PathBuf::from(format!("{}{}", parent.path, stage_name));
    let mut stage = relative(
        parent.last(),
        &stage_name,
        false,
        true,
        true,
        FILE_SHARE_READ,
    )?;
    let mut published = false;
    let result = (|| {
        info(&stage)?;
        faults.check("write")?;
        #[cfg(test)]
        if faults.point == Some("partial") {
            stage.write_all(&copy.edited().bytes()[..16]).unwrap();
            return Err(ExportError::Injected("partial write failure"));
        }
        write_stream(&mut stage, copy.edited().bytes())?;
        faults.check("cleanup")?;
        stage
            .sync_all()
            .map_err(|e| ExportError::io("sync staging contents", e))?;
        #[cfg(test)]
        if faults.point == Some("corrupt") {
            stage.seek(SeekFrom::Start(0)).unwrap();
            stage.write_all(&[0]).unwrap();
        }
        let staged = read_stream(&mut stage, copy.edited().bytes().len())?;
        compare(&staged, copy.edited().bytes())?;
        faults.check("verify")?;
        verify(copy, current, env, diagnostic, &staged)?;
        faults.check("publish")?;
        #[cfg(test)]
        if faults.point == Some("collision") {
            let mut rival = relative(
                parent.last(),
                &parent.name,
                false,
                true,
                true,
                FILE_SHARE_READ,
            )?;
            rival.write_all(b"existing sentinel").unwrap();
        }
        rename_no_replace(&stage, &parent.name)?;
        published = true;
        faults.check("post")?;
        // Reopen by the retained parent handle, then compare physical identity.
        // Share all to coexist with our writer; its READ-only share still denies
        // third-party writable/deletable opens until postverification completes.
        let mut reopened = relative(
            parent.last(),
            &parent.name,
            false,
            false,
            false,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
        )?;
        if !same_file(&stage, &reopened)? || info(&reopened)?.nNumberOfLinks != 1 {
            return Err(ExportError::UnsafeRedirection);
        }
        let bytes = read_stream(&mut reopened, copy.edited().bytes().len())?;
        compare(&bytes, copy.edited().bytes())?;
        verify(copy, current, env, diagnostic, &bytes)?;
        Ok(())
    })();
    match result {
        Ok(()) => Ok(()),
        Err(error) => {
            let cleanup_error = if published {
                None
            } else {
                // A cleanup fault is test-only; production always attempts it.
                faults.check("cleanup").and_then(|_| cleanup(&stage)).err()
            };
            Err(ExportFailure {
                error,
                publication: if published {
                    Publication::PublishedReadbackFailed
                } else {
                    Publication::NotPublished
                },
                staging_path: if published { None } else { Some(stage_path) },
                cleanup_error,
            })
        }
    }
}
