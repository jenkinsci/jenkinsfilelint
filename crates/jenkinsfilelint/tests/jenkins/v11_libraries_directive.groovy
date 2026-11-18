pipeline {
    agent any
    libraries {
        lib('my-lib@1.0')
    }
    stages {
        stage('S') {
            steps { echo 'x' }
        }
    }
}
