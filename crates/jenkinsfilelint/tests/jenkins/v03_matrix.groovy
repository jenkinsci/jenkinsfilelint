pipeline {
    agent none
    stages {
        stage('BuildAndTest') {
            matrix {
                agent any
                axes {
                    axis {
                        name 'PLATFORM'
                        values 'linux', 'windows', 'mac'
                    }
                    axis {
                        name 'BROWSER'
                        values 'firefox', 'chrome', 'safari'
                    }
                }
                excludes {
                    exclude {
                        axis {
                            name 'PLATFORM'
                            values 'linux'
                        }
                        axis {
                            name 'BROWSER'
                            notValues 'chrome', 'firefox'
                        }
                    }
                }
                stages {
                    stage('Build') {
                        steps { echo "Build ${PLATFORM} ${BROWSER}" }
                    }
                }
            }
        }
    }
}
