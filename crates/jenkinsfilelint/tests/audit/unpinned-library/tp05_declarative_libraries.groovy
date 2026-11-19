pipeline {
    agent { label 'linux' }
    options {
        timeout(time: 1, unit: 'HOURS')
    }
    libraries {
        lib('shared@main')
        lib('utils@1.4.0')
    }
    stages {
        stage('Build') {
            steps {
                sh 'make'
            }
        }
    }
}
