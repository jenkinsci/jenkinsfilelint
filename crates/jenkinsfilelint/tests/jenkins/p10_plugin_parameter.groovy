pipeline {
    agent any
    parameters {
        gitParameter(name: 'BRANCH', type: 'PT_BRANCH')
    }
    stages {
        stage('S') {
            steps { echo 'x' }
        }
    }
}
