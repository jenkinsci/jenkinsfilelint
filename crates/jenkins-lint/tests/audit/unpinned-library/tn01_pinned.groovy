@Library('shared@v1.4.0') _
@Library(['utils@2.0.0-rc.1', 'deploy@3']) _
@Library('tools@4f2a9c1e8b7d6a5f4e3d2c1b0a9f8e7d6c5b4a39') _

library 'docs@refs/tags/stable'
library identifier: 'release@1.2.3', retriever: modernSCM(
    [$class: 'GitSCMSource', remote: 'https://example.com/release.git'])

// The library's own repository and branch, as in its own tests.
def lib = library identifier: 'sct@snapshot', retriever: legacySCM(scm)

// A placeholder that a template fills in.
@Library('ods-jenkins-shared-library@@shared_library_ref@') _
