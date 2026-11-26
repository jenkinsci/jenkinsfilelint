pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                parallel([a: { echo 'x' }, b: { echo 'y' }])
            }
        }
    }
}
