pipeline {
    agent any
    options {
        preserveStashes(buildCount: 5)
        quietPeriod(30)
        checkoutToSubdirectory('src')
        newContainerPerStage()
        parallelsAlwaysFailFast()
    }
    stages {
        stage('S') {
            steps { echo 'x' }
        }
    }
}
