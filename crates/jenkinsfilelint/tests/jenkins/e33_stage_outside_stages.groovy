pipeline {
    agent any
    stage('S') {
        steps { echo 'x' }
    }
}
